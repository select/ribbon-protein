# Public DSSP regression backbones

These are deliberately minimal **public RCSB PDB-derived** fixtures, not copies of
local/private input bundles. Original public downloads were retrieved and checked
from the following HTTPS URLs before extraction:

| Entry | Source | SHA-256 of complete downloaded PDB |
| --- | --- | --- |
| 1EZG | <https://files.rcsb.org/download/1EZG.pdb> | `7e274b0fcdc51762b6655fafc516122dcc5fd3b3277059636fdbbd57c7d6fb83` |
| 1TEN | <https://files.rcsb.org/download/1TEN.pdb> | `5b2165e4e495d05e230c6438a8585966578fc70006ec228ac0a0d8b65d06c73c` |
| 1CRN | <https://files.rcsb.org/download/1CRN.pdb> | `42199a30a0701864a2a5cc76cd7f35cc544cd0e65fbcf63e03c166543249b811` |
| 1MBN | <https://files.rcsb.org/download/1MBN.pdb> | `e4635115be5a605e58f449f82824e15182a7000c3677876c33fc44118defef78` |

RCSB entry pages provide the experimental provenance, primary citations and
wwPDB data policy: <https://www.rcsb.org/structure/1EZG>,
<https://www.rcsb.org/structure/1TEN>, <https://www.rcsb.org/structure/1CRN>,
<https://www.rcsb.org/structure/1MBN>. Cite those entries for scientific reuse.

## Reproduce extraction

Download the four URLs to a temporary directory with lowercase filenames
`1ezg.pdb`, `1ten.pdb`, `1crn.pdb`, `1mbn.pdb`, verify the hashes, then run:

```sh
node scripts/extract-dssp-fixtures.mjs /path/to/downloads
```

The script retains only first-model ATOM N/CA/C/O/OXT and TER records. It retains
residue names (including PRO), numbering, author chains, coordinates, occupancies
and alternate locations. It removes all HELIX/SHEET tags and unrelated atoms.
1CRN is placed in `public/examples/` for the offline demo; its generated mmCIF
has the same coordinates/residues but deliberately renames author chain A to
`crambin.original` and label chain to `L0` to test identity preservation.

Tests do **not** download anything. The existing synthetic CA examples remain
useful topology/renderer data, but their illustrative N/C/O positions are not a
hydrogen-bond DSSP oracle. The parser tests instead perturb physically consistent
1CRN coordinates, explicitly using ALA mutations only in topology tests.

## Expectations and independent comparison

The 1EZG exact simplified label string (each 82-residue chain) was independently
verified with **MDTraj 1.11.2**, using
`compute_dssp(load("1ezg.pdb"), simplified=True)[0]` on the complete public PDB.
It matches the app exactly: H=0/E=38/C=126 total, including six EEE runs per chain.
1CRN also matches independently: H=22/E=4/C=20. Its final three-residue helix is
raw DSSP G (3-10 helix), exercising G → H simplification. Replacing PRO with ALA
incorrectly permits an extra donor and extends the first helix to residue 18;
the chemistry regression test detects that normalization error.

1TEN/1MBN tests freeze the **app's documented Mol* original-convention result**,
not exact MDTraj labels: 1TEN H=0/E=51/C=38 (89 drawable residues); 1MBN
H=118/E=0/C=35 (153 residues). Independent MDTraj results for the same drawable
residues are 0/50/39 and 120/0/33 respectively. See
[the implementation notes](../../../docs/web-dssp.md) for the specific differing
residues and why global DSSP parity is not claimed.

The inter-chain test separates 1CRN residues 1–4 and 32–35 into two chains: both
have an EE run together, neither has E when isolated. A three-residue partner
still supplies a genuine contact without becoming a drawable ribbon. Other
perturbations cover missing atoms, peptide/CA/sequence gaps, coherent occupancy
selection, lexical ties, first-model choice and blank author chain IDs.
