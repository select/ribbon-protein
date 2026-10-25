"""Regression coverage for polymer/solvent chain splitting during PDB interchange."""
import tempfile
import unittest
from pathlib import Path

import gemmi

from render import load_structure


def write_structure_with_waters(path):
    structure = gemmi.Structure()
    model = gemmi.Model('1')
    for name, y in [('target-long', 0), ('binder-long', 15)]:
        chain = gemmi.Chain(name)
        for i in range(6):
            residue = gemmi.Residue()
            residue.name = 'ALA'
            residue.seqid = gemmi.SeqId(i + 1, ' ')
            residue.het_flag = 'A'
            residue.entity_type = gemmi.EntityType.Polymer
            for atom_name, dx, dy, element in [('N', -1.1, 0.4, 'N'), ('CA', 0, 0, 'C'),
                                               ('C', 1.2, 0.3, 'C'), ('O', 1.4, 1.4, 'O')]:
                atom = gemmi.Atom()
                atom.name = atom_name
                atom.element = gemmi.Element(element)
                atom.pos = gemmi.Position(i * 3.8 + dx, y + dy, 0)
                residue.add_atom(atom)
            chain.add_residue(residue)
        water = gemmi.Residue()
        water.name = 'HOH'
        water.seqid = gemmi.SeqId(100, ' ')
        water.het_flag = 'H'
        water.entity_type = gemmi.EntityType.Water
        atom = gemmi.Atom()
        atom.name = 'O'
        atom.element = gemmi.Element('O')
        atom.pos = gemmi.Position(5, y + 10, 10)
        water.add_atom(atom)
        chain.add_residue(water)
        model.add_chain(chain)
    structure.add_model(model)
    structure.setup_entities()
    structure.make_mmcif_document().write_file(str(path))


class StructureLoadingTests(unittest.TestCase):
    def test_waters_do_not_truncate_or_mislabel_later_protein_chains(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'solvated.cif'
            write_structure_with_waters(path)
            segments, counts = load_structure(path, None)
            self.assertEqual(sum(counts.values()), 12)
            self.assertEqual([(segment[0][3], len(segment)) for segment in segments],
                             [('target-long', 6), ('binder-long', 6)])

    def test_original_long_chain_selection_survives_solvent_splitting(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'solvated.cif'
            write_structure_with_waters(path)
            segments, counts = load_structure(path, ['binder-long'])
            self.assertEqual(sum(counts.values()), 6)
            self.assertEqual([(segment[0][3], len(segment)) for segment in segments],
                             [('binder-long', 6)])

    @unittest.skipUnless(Path('output/structures/1axc-assembly1.cif').exists(), 'Local PCNA assembly unavailable')
    def test_pcna_assembly_includes_all_three_subunits_and_bound_peptides(self):
        segments, counts = load_structure(Path('output/structures/1axc-assembly1.cif'), None)
        self.assertEqual(sum(counts.values()), 804)
        self.assertEqual({segment[0][3] for segment in segments}, {'A', 'B', 'C', 'D', 'E', 'F'})


if __name__ == '__main__':
    unittest.main()
