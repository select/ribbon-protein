# Offline structure examples

**Load example** uses `1crn-backbone.pdb`: a minimal first-model backbone from the
public RCSB **1CRN** structure (crambin). `1crn-backbone.cif` contains the same
physical coordinates with a deliberately multi-character author chain ID.
Neither file contains HELIX/SHEET tags. Local hydrogen-bond DSSP recognizes both
helices and a small beta sheet (H=22/E=4/C=20); no database request is needed.

These files retain N/CA/C/O/OXT, original residue chemistry, coordinates and
occupancies. See [public source URLs, hashes, extraction instructions and DSSP
expectations](../../tests/fixtures/dssp/README.md). Reproduce them with
`scripts/extract-dssp-fixtures.mjs` after obtaining the documented public sources.

`synthetic.pdb` and `synthetic.cif` are older locally generated illustrative CA
helix/strand examples, produced by `node scripts/generate-fixtures.mjs`. Their
C/O/N coordinates are **not physically appropriate for hydrogen-bond DSSP**.
They remain topology/rendering examples only, not scientific assignment fixtures
or the studio's default demo. Do not use them for scientific analysis.

Downloaded/private user structures belong in ignored local folders, not here.
The committed scientific demo is an explicitly verified public-data exception.
