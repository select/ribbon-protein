import type { RenderResult } from "../types/render";

/** The renderer creates SVG, not user-provided markup. Still enforce a strict
 * line-only subset before download or rasterization, including URL rejection.
 */
export function validateSvg(svg: string): void {
  if (
    !/^\s*(?:<\?xml[^>]*>\s*)?<svg[\s>]/.test(svg) ||
    /<!|<\?|(?:href|src)\s*=|\bon\w+\s*=|url\s*\(/i.test(svg.replace(/^\s*<\?xml[^>]*>/, "")) ||
    svg.includes(String.fromCharCode(0))
  )
    throw new Error("Renderer SVG contains unsupported or unsafe markup.");
  const tags = svg.match(/<\/?[\w:-]+\b/g) ?? [];
  if (tags.some((tag) => !/^<\/?(?:svg|g|path|polyline|line|title|desc)$/.test(tag)))
    throw new Error(
      "Export must contain only line paths, without fills, images, masks or clipping.",
    );
  const fills = [...svg.matchAll(/\bfill\s*=\s*["']([^"']*)["']/gi)];
  if (!fills.length || fills.some((match) => match[1] !== "none"))
    throw new Error("Renderer SVG is not line-only (fill must be none).");
  if (/\bstyle\s*=/i.test(svg)) throw new Error("Inline SVG styles are not permitted.");
}
export function exportFileName(name: string, extension: "svg" | "png"): string {
  const stem = name
    .replace(/\.(pdb|ent|cif|mmcif)$/i, "")
    .replace(/[^\p{L}\p{N}._-]+/gu, "-")
    .replace(/^[.-]+|[.-]+$/g, "")
    .slice(0, 100);
  return `${stem || "ribbon"}.${extension}`;
}
export function svgBlob(result: RenderResult): Blob {
  validateSvg(result.svg);
  return new Blob([result.svg], { type: "image/svg+xml;charset=utf-8" });
}
export function saveBlob(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.style.display = "none";
  document.body.append(anchor);
  try {
    anchor.click();
  } finally {
    anchor.remove();
    // Firefox needs the URL alive until the browser has consumed the click.
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}
export function pngSize(
  widthMm: number,
  heightMm: number,
  dpi = 150,
): { width: number; height: number } {
  if (![widthMm, heightMm, dpi].every((v) => Number.isFinite(v) && v > 0))
    throw new Error("PNG dimensions and resolution must be finite positive numbers.");
  const width = Math.ceil((widthMm / 25.4) * dpi);
  const height = Math.ceil((heightMm / 25.4) * dpi);
  if (width * height > 32_000_000 || width > 16384 || height > 16384)
    throw new Error("PNG would be too large. Use SVG or smaller paper dimensions.");
  return { width, height };
}
export async function pngBlob(result: RenderResult): Promise<Blob> {
  const { width, height } = pngSize(result.widthMm, result.heightMm);
  const url = URL.createObjectURL(svgBlob(result));
  try {
    const image = new Image();
    image.src = url;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("This browser cannot create a PNG canvas. Use SVG instead.");
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, width, height);
    ctx.drawImage(image, 0, 0, width, height);
    return await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (blob) => (blob ? resolve(blob) : reject(new Error("PNG encoding failed."))),
        "image/png",
      ),
    );
  } finally {
    URL.revokeObjectURL(url);
  }
}
