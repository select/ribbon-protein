#!/usr/bin/env python3
"""Crop EGFR domain III, rigidly superpose complexes, and record a shared frame."""
import argparse
import copy
import json
from pathlib import Path

import gemmi
import numpy as np
from scipy.spatial.transform import Rotation

from render import camera, load_structure, make_ribbon


def protein_residues(chain):
    return [residue for residue in chain if 'CA' in residue and
            gemmi.find_tabulated_residue(residue.name).is_amino_acid()]


def sequence(residues):
    return ''.join(gemmi.find_tabulated_residue(r.name).one_letter_code for r in residues)


def rigid_fit(mobile, reference):
    """Return proper row-vector rotation/translation fitting matched coordinates."""
    mobile = np.asarray(mobile, dtype=float)
    reference = np.asarray(reference, dtype=float)
    if mobile.shape != reference.shape or mobile.ndim != 2 or mobile.shape[1] != 3:
        raise ValueError('Expected paired N x 3 coordinate arrays')
    a, b = mobile.mean(axis=0), reference.mean(axis=0)
    u, _, vt = np.linalg.svd((mobile - a).T @ (reference - b))
    correction = np.eye(3)
    correction[-1, -1] = np.linalg.det(u @ vt)
    rotation = u @ correction @ vt
    translation = b - a @ rotation
    rmsd = np.sqrt(np.mean(np.sum((mobile @ rotation + translation - reference) ** 2, axis=1)))
    return rotation, translation, float(rmsd)


def ca_coordinates(residues):
    return np.array([[r['CA'][0].pos.x, r['CA'][0].pos.y, r['CA'][0].pos.z] for r in residues])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=Path('blender_message_top40'))
    parser.add_argument('--output', type=Path, default=Path('output/domain3-aligned-inputs'))
    parser.add_argument('--reference', default='rank10')
    parser.add_argument('--rotate-view', nargs=3, type=float, default=[0, 0, 0],
                        metavar=('X', 'Y', 'Z'), help='Right-handed rotation in reference-camera axes (degrees)')
    args = parser.parse_args()
    entries = sorted(json.loads((args.input / 'manifest.json').read_text()), key=lambda e: e['rank'])
    reference_entry = next(e for e in entries if e['rank'] == args.reference)
    reference_structure = gemmi.read_structure(str(args.input / reference_entry['file']))
    reference_residues = protein_residues(reference_structure[0][reference_entry['target_chain']])
    reference_sequence = sequence(reference_residues)
    if len(reference_sequence) != 172:
        raise ValueError('Reference must be the 172-residue domain III crop')
    reference_ca = ca_coordinates(reference_residues)
    camera_center, camera_matrix = camera(reference_ca, [0, 0, 0])
    camera_matrix = Rotation.from_euler('xyz', args.rotate_view, degrees=True).as_matrix() @ camera_matrix
    args.output.mkdir(parents=True, exist_ok=True)
    prepared, reports, bounds = [], [], []
    for entry in entries:
        source = gemmi.read_structure(str(args.input / entry['file']))
        target = protein_residues(source[0][entry['target_chain']])
        target_sequence = sequence(target)
        start = target_sequence.find(reference_sequence)
        if start < 0 or target_sequence.find(reference_sequence, start + 1) >= 0:
            raise ValueError(f"{entry['rank']}: reference domain sequence must match exactly once")
        domain = target[start:start + len(reference_sequence)]
        rotation, translation, rmsd = rigid_fit(ca_coordinates(domain), reference_ca)
        # The same proper rigid transform acts on every atom in BOTH partners.
        combined_rotation = rotation @ camera_matrix.T
        combined_translation = (translation - camera_center) @ camera_matrix.T
        selected = gemmi.Structure()
        model = gemmi.Model('1')
        chain = gemmi.Chain(entry['target_chain'])
        for number, residue in enumerate(domain, 1):
            residue = residue.clone()
            residue.seqid = gemmi.SeqId(number, ' ')
            chain.add_residue(residue)
        model.add_chain(chain)
        binder = source[0][entry['binder_chain']].clone()
        model.add_chain(binder)
        selected.add_model(model)
        for chain in selected[0]:
            for residue in chain:
                for atom in residue:
                    xyz = np.array([atom.pos.x, atom.pos.y, atom.pos.z])
                    xyz = xyz @ combined_rotation + combined_translation
                    atom.pos = gemmi.Position(*xyz)
        destination = args.output / f"{entry['rank']}-domain3-aligned.pdb"
        selected.write_pdb(str(destination))
        aligned = gemmi.read_structure(str(destination))
        aligned_ca = ca_coordinates(protein_residues(aligned[0][entry['target_chain']]))
        expected = (reference_ca - camera_center) @ camera_matrix.T
        written_rmsd = float(np.sqrt(np.mean(np.sum((aligned_ca - expected) ** 2, axis=1))))
        if not np.isclose(written_rmsd, rmsd, atol=0.002):
            raise ValueError('Written structure does not preserve the fitted frame')
        record = copy.deepcopy(entry)
        record.update(file=destination.name, target_residues=172,
                      original_file=entry['file'], original_target_residues=len(target),
                      structure_type='predicted human EGFR domain III complex; rigidly aligned')
        # Original hashes describe source files, not these transformed derivatives.
        record.pop('sha256', None)
        prepared.append(record)
        reports.append({'rank': entry['rank'], 'reference': args.reference,
                        'matched_ca_atoms': len(domain), 'rmsd_angstrom': rmsd,
                        'source_domain_first_seqid': str(domain[0].seqid),
                        'source_domain_last_seqid': str(domain[-1].seqid),
                        'rotation_row_vector': combined_rotation.tolist(),
                        'translation_angstrom': combined_translation.tolist()})
        segments, _ = load_structure(destination, None)
        for segment in segments:
            points, _, _, width, _ = make_ribbon(segment, 16)
            bounds.extend([points[:, :2] - width[:, None], points[:, :2] + width[:, None]])
        print(f"{entry['rank']}: {len(target)} -> 172 target residues; C-alpha RMSD {rmsd:.3f} A", flush=True)
    all_bounds = np.concatenate(bounds)
    low, high = all_bounds.min(axis=0) - 0.5, all_bounds.max(axis=0) + 0.5
    view_bounds = [*low.tolist(), *high.tolist()]
    (args.output / 'manifest.json').write_text(json.dumps(prepared, indent=2) + '\n')
    (args.output / 'alignment-report.json').write_text(json.dumps({
        'reference': args.reference, 'reference_sequence': reference_sequence,
        'view_rotation_degrees_xyz': args.rotate_view,
        'rotation_convention': 'Right-handed active rotation in reference-camera axes; positive Y is counterclockwise viewed from +Y toward origin',
        'method': 'Exact sequence correspondence; unweighted least-squares C-alpha Kabsch fit; proper rigid transform applied to both partners',
        'view_bounds': view_bounds, 'entries': reports}, indent=2) + '\n')
    print('Shared view bounds:', *view_bounds)


if __name__ == '__main__':
    main()
