# Local DSSP assignment and the sheet-ribbon regression

## Root cause and scope

The initial web parser used Mol* 5.11 Zhang–Skolnick CA geometry. On 1EZG it
assigned all 164 residues as C, while the reference DSSP assignment has 38 E.
Because the renderer uses E width 1.35 Å versus C width 0.20 Å (with dense loop
tracks), this produced narrow dark tubes instead of broad sheet ribbons.
Changing stroke width, camera or forcing sheet labels would not fix the cause.

The corrected worker uses **hydrogen-bond DSSP**, retaining the Rust geometry,
PCA convention, ink defaults and client-only architecture. It does not depend on
file HELIX/SHEET annotations or a hosted service. Selection, assignment and WASM
rendering all remain inside the cancellable, two-minute worker job.

## Mol* API and chemical/topological adapter

`app/lib/dssp.ts` uses the pinned Mol* 5.11 DSSP original-convention stages.
Inspection of `computeUnitDSSP(unit, params)` and its implementation showed that
calling it unchanged is not sufficient for normalized, fragmented input:

- Its H reconstruction uses the preceding array entry even across chains/gaps.
- It does not exclude PRO as an NH donor.
- Its sequence arithmetic for turns/bridges/ladders assumes a continuous array.
- `Structure.ofModel()` may partition separate chains into distinct units, losing
  inter-chain contacts if each is computed independently.

The adapter therefore builds one atomic unit containing all selected backbones
and uses Mol*'s protein-info and 9 Å CA spatial lookup. It reconstructs a 1 Å
amide H from the previous peptide C–O direction only within a continuous
fragment, never for PRO. The Kabsch–Sander electrostatic expression uses
Q=-27.888, energy cutoff -0.5 kcal/mol and lower cap -9.9, as in Mol*. OXT-marked
C termini are excluded as acceptors, following Mol*'s convention. Explicit H
naming in input does not affect the conventional reconstructed hydrogen.

Mol*'s `assignTurns`/`assignHelices` run per continuous fragment; beta bridges use
its reciprocal/offset H-bond patterns with explicit peptide-neighbor checks.
Mol* ladder/sheet stages run per fragment-pair so bulge filling cannot jump a
break. Inter-strand/inter-chain hydrogen bonds remain in the shared graph.
Bends/dihedrals are unnecessary for simplified C and are not computed.

`DefaultDSSPComputationProps` has **oldDefinition=true, oldOrdering=true**:
original, shorter helix boundaries with alpha preferred over 3-10. The winning
raw DSSP priority is H > E > B > G > I > T > S. Simplification is **H/G/I → H,
E/B → E, everything else → C**, matching the reference's simplified categories.
Mol* internal DSSP bits are not `SecondaryStructureType.Flag` bits; the adapter
names their pinned numeric values explicitly because TypeScript's verbatim
module mode cannot access ambient const enums.

The parser preserves each selected residue's chemical component ID instead of
replacing every residue with ALA. It selects one complete highest-occupancy
conformer (lexical ties), reuses shared blank-alt atoms, and keeps coordinates
unchanged. First model, original author chain IDs, polymer-only selection and
finite complete N/CA/C/O filtering remain enforced. Optional OXT is retained.
Fragments split at missing atoms, sequence/CA gaps, duplicate CA or peptide C–N
distances >2.5 Å; CA distances >4.5 Å also split. Short fragments supply DSSP
contacts but only fragments of four or more residues are drawn.

Existing file/atom/residue/fragment limits remain. DSSP additionally caps contact
candidates at 2,000,000, H-bonds at 100,000 and estimated ladder search work at
6,000,000. Failures are actionable errors, never an all-loop fallback. Worker
termination still cancels parsing/assignment/rendering without uploading input.

## Public references and independent oracle

Minimal, unannotated public RCSB fixtures have [explicit provenance and
reproduction instructions](../tests/fixtures/dssp/README.md). No ignored local
structure bundles or downloaded renders are committed.

Counts below use the same drawable complete residues; columns are H/E/C:

| Public structure | Superseded CA geometry | Corrected DSSP | MDTraj 1.11.2 independent oracle |
| --- | --- | --- | --- |
| 1EZG, 164 residues / 2 fragments | 0/0/164 | **0/38/126** | **0/38/126**, exact per-residue match |
| 1TEN, 89 residues | 0/25/64 | **0/51/38** | 0/50/39 |
| 1CRN, 46 residues, mixed | 15/3/28 | **22/4/20** | **22/4/20**, exact per-residue match |
| 1MBN, 153 residues, helix-rich | 96/1/56 | **118/0/35** | 120/0/33 |

The actual local `1ezg-assembly1.cif` also yields 0/38/126. Each chain contains
six continuous EEE runs, not just isolated E labels. The independent oracle is
`mdtraj.compute_dssp(mdtraj.load(public_pdb), simplified=True)[0]`; raw labels
were also inspected. This is local validation tooling, not a web dependency.

Remaining differences were inspected rather than hidden:

- **1TEN:** A803 LEU is oracle B/E but app C: preceding residue A802 lacks N and
  is excluded before app assignment, removing its peptide predecessor. A883 ASN
  and A884 PRO are oracle coil but Mol* E from beta-bulge/sheet extension. PRO
  can participate in a sheet as an acceptor; it must not donate an amide H.
- **1MBN:** A96 LYS and A97 HIS are oracle I/H (pi helix) but Mol* original
  helix-priority/boundary logic leaves them C. The unmodified Mol* routine also
  gives 118/0/35 here. No other simplified labels differ.
- **1CRN:** the unmodified Mol* routine gives one extra helix residue because it
  treats PRO as a donor. The adapter removes that error, matching the oracle.

This is not a promise of exact parity with every DSSP release: hydrogen-bond
selection, helix priorities and beta-bulge extension differ among implementations.
Unusual modified-residue chemistry and incomplete backbones need particular
care. Mol* itself documents outstanding sheet-extension/turn limitations.
Average-panel-depth occlusion and PCA view differences are separate renderer
limitations, not secondary-structure assignment failures.
