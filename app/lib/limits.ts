export const LIMITS = {
  fileBytes: 20 * 1024 * 1024,
  atoms: 250_000,
  residues: 10_000,
  segments: 512,
  renderWork: 6_000_000,
  dsspPairs: 2_000_000,
  dsspBonds: 100_000,
  jobTimeoutMs: 120_000,
} as const;

export function validateFile(name: string, bytes: number): void {
  if (name.length > 1024) throw new Error("Structure filename exceeds 1024 characters.");
  if (!/\.(pdb|ent|cif|mmcif)$/i.test(name))
    throw new Error("Choose a .pdb, .ent, .cif or .mmcif structure file.");
  if (!bytes) throw new Error("The structure file is empty.");
  if (bytes > LIMITS.fileBytes)
    throw new Error(
      "The structure exceeds the 20 MiB local file limit. Export a smaller first-model protein.",
    );
}
