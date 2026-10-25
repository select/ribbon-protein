// Reproduce the minimal public fixtures from RCSB PDB downloads. No network at
// application runtime/build/test. See tests/fixtures/dssp/README.md for hashes.
// Usage: node scripts/extract-dssp-fixtures.mjs /directory/with/1ezg.pdb-etc
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
const directory = process.argv[2];
if (!directory) throw new Error("Provide a directory containing the four RCSB PDB downloads.");
const header = [
  "group_PDB",
  "id",
  "type_symbol",
  "label_atom_id",
  "label_alt_id",
  "label_comp_id",
  "label_asym_id",
  "label_entity_id",
  "label_seq_id",
  "pdbx_PDB_ins_code",
  "Cartn_x",
  "Cartn_y",
  "Cartn_z",
  "occupancy",
  "B_iso_or_equiv",
  "auth_seq_id",
  "auth_comp_id",
  "auth_asym_id",
  "auth_atom_id",
  "pdbx_PDB_model_num",
];
await mkdir(new URL("../tests/fixtures/dssp/", import.meta.url), { recursive: true });
const sources = {
  "1ezg": "7e274b0fcdc51762b6655fafc516122dcc5fd3b3277059636fdbbd57c7d6fb83",
  "1ten": "5b2165e4e495d05e230c6438a8585966578fc70006ec228ac0a0d8b65d06c73c",
  "1crn": "42199a30a0701864a2a5cc76cd7f35cc544cd0e65fbcf63e03c166543249b811",
  "1mbn": "e4635115be5a605e58f449f82824e15182a7000c3677876c33fc44118defef78",
};
for (const [id, expectedHash] of Object.entries(sources)) {
  const source = await readFile(resolve(directory, `${id}.pdb`), "utf8");
  if (createHash("sha256").update(source).digest("hex") !== expectedHash)
    throw new Error(`Source hash mismatch for ${id}: use the documented public RCSB download.`);
  const lines = [];
  for (const line of source.split("\n")) {
    if (line.startsWith("ENDMDL")) break;
    if (
      line.startsWith("TER") ||
      (line.startsWith("ATOM  ") &&
        ["N", "CA", "C", "O", "OXT"].includes(line.slice(12, 16).trim()))
    )
      lines.push(line.trimEnd());
  }
  const text = `HEADER    RCSB ${id.toUpperCase()} FIRST-MODEL BACKBONE\nREMARK    N CA C O OXT ONLY; SEE FIXTURE README FOR PROVENANCE\n${lines.join("\n")}\nEND\n`;
  const target =
    id === "1crn"
      ? `../public/examples/${id}-backbone.pdb`
      : `../tests/fixtures/dssp/${id}-backbone.pdb`;
  await writeFile(new URL(target, import.meta.url), text);
  if (id === "1crn") {
    const rows = lines
      .filter((l) => l.startsWith("ATOM"))
      .map((l) => {
        const atom = l.slice(12, 16).trim(),
          comp = l.slice(17, 20).trim(),
          seq = l.slice(22, 26).trim();
        return `ATOM ${l.slice(6, 11).trim()} ${atom[0]} ${atom} ${l[16].trim() || "."} ${comp} L0 1 ${seq} ${l[26].trim() || "?"} ${l.slice(30, 38).trim()} ${l.slice(38, 46).trim()} ${l.slice(46, 54).trim()} ${l.slice(54, 60).trim()} ${l.slice(60, 66).trim()} ${seq} ${comp} crambin.original ${atom} 1`;
      });
    await writeFile(
      new URL("../public/examples/1crn-backbone.cif", import.meta.url),
      `data_1crn_backbone\n# RCSB 1CRN coordinates; author chain renamed to exercise chain preservation.\nloop_\n${header.map((h) => `_atom_site.${h}`).join("\n")}\n${rows.join("\n")}\n`,
    );
  }
}
