// Deterministic, synthetic CA helix/extended strand. Not an experimental protein.
// C/O/N are illustrative complete-backbone coordinates, not energy minimized.
import { writeFile } from "node:fs/promises";
const add = (a, b) => a.map((v, i) => v + b[i]);
export function syntheticResidues(kind, count = 16) {
  return Array.from({ length: count }, (_, i) => {
    const angle = (i * 100 * Math.PI) / 180;
    const ca =
      kind === "H"
        ? [2.3 * Math.cos(angle), 2.3 * Math.sin(angle), i * 1.5]
        : [i * 3.3 - 20, i % 2 ? 15.65 : 14.35, 0];
    const c = add(ca, [0.7, 0.5, 0]);
    return { N: add(ca, [-0.7, -0.5, -0.2]), CA: ca, C: c, O: add(c, [0, 0.6, 0.9]) };
  });
}
export function pdbAtom(
  serial,
  atom,
  seq,
  chain,
  point,
  alt = " ",
  occupancy = 1,
  insertion = " ",
) {
  return `ATOM  ${String(serial).padStart(5)} ${atom.padStart(3)} ${alt}ALA ${chain}${String(seq).padStart(4)}${insertion}   ${point.map((v) => v.toFixed(3).padStart(8)).join("")}${occupancy.toFixed(2).padStart(6)} 20.00          ${atom[0].padStart(2)}  `;
}
if (process.argv[1] && import.meta.url === new URL(process.argv[1], "file:").href) {
  const pdb = [
    "HEADER    SYNTHETIC BACKBONE EXAMPLE",
    "REMARK    IDEAL CA GEOMETRY; NOT AN EXPERIMENTAL STRUCTURE",
  ];
  const cif = [
    "data_synthetic",
    "# Ideal synthetic CA geometry, not experimental coordinates.",
    "loop_",
    ...[
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
    ].map((s) => `_atom_site.${s}`),
  ];
  let id = 0;
  for (const [index, kind] of ["H", "E"].entries()) {
    const residues = syntheticResidues(kind);
    residues.forEach((residue, r) =>
      Object.entries(residue).forEach(([atom, point]) => {
        id++;
        pdb.push(pdbAtom(id, atom, r + 1, kind, point));
        const auth = kind === "H" ? "helix-original" : "strand.original";
        cif.push(
          `ATOM ${id} ${atom[0]} ${atom} . ALA L${index} ${index + 1} ${r + 1} ? ${point.map((v) => v.toFixed(3)).join(" ")} 1 20 ${r + 101} ALA ${auth} ${atom} 1`,
        );
      }),
    );
    pdb.push("TER");
  }
  pdb.push("END");
  await writeFile(
    new URL("../public/examples/synthetic.pdb", import.meta.url),
    pdb.map((line) => line.trimEnd()).join("\n") + "\n",
  );
  await writeFile(
    new URL("../public/examples/synthetic.cif", import.meta.url),
    cif.join("\n") + "\n",
  );
}
