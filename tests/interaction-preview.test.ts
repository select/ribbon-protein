import { describe, expect, it } from "vitest";
import type { PreviewScene } from "../app/types/render";
import { defaultRenderOptions } from "../app/types/render";
import { previewGeometryKey, renderInteractionPreview } from "../app/lib/interaction-preview";
import { svgBlob, validateSvg } from "../app/lib/exports";

const scene: PreviewScene = {
  center: [0, 0, 0],
  basis: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
  ribbons: [
    {
      chainId: "A",
      samples: Array.from({ length: 12 }, (_, i) => ({
        point: [Math.sin(i), i, Math.cos(i)] as [number, number, number],
        side: [1, 0, 0],
        width: 1.35,
        alpha: 0,
      })),
    },
  ],
};
describe("bounded display-only prepared 3D preview", () => {
  it("rotates real 3D geometry, not a 2D image, and never mutates its reusable scene", () => {
    const before = structuredClone(scene);
    const options = defaultRenderOptions();
    const first = renderInteractionPreview(scene, options);
    options.rotation = [30, 45, 12];
    const rotated = renderInteractionPreview(scene, options);
    expect(rotated.svg).not.toBe(first.svg);
    expect(rotated.quality).toBe("preview");
    expect(rotated.svg).toContain("Hidden strokes are NOT removed");
    expect(rotated.strokeCount).toBe(2);
    expect(rotated.lengthMm).toBeGreaterThan(0);
    expect(scene).toEqual(before);
    validateSvg(rotated.svg);
    expect(() => svgBlob(rotated)).toThrow(/cannot be exported/);
  });
  it("preserves half-pen page clearance, custom sizes and chain colors", () => {
    const options = {
      ...defaultRenderOptions(),
      widthMm: 93.14,
      heightMm: 93.14,
      marginMm: 5,
      penWidthMm: 2,
      chainColors: { A: "#123aBC" },
    };
    const result = renderInteractionPreview(scene, options);
    expect(result.svg).toContain('stroke="#123aBC"');
    expect(result.svg).toContain('width="93.14mm" height="93.14mm"');
    for (const match of result.svg.matchAll(/[ML]([\d.-]+),([\d.-]+)/g)) {
      expect(Number(match[1])).toBeGreaterThanOrEqual(5.9999);
      expect(Number(match[2])).toBeGreaterThanOrEqual(5.9999);
      expect(Number(match[1])).toBeLessThanOrEqual(87.1401);
      expect(Number(match[2])).toBeLessThanOrEqual(87.1401);
    }
  });
  it("previews bond widths as real outlines, handles end-on bonds, and keeps atoms as circles", () => {
    const molecular: PreviewScene = {
      ...scene,
      ribbons: [],
      molecules: [
        {
          chainId: "lipid",
          label: "lipid",
          category: "lipid",
          atoms: [
            [0, 0, 0],
            [2, 0, 0],
            [0, 4, 0],
          ],
          bonds: [[0, 1]],
        },
      ],
    };
    const o = defaultRenderOptions();
    o.selectedChains = ["lipid"];
    const before = structuredClone(molecular);
    const band = renderInteractionPreview(molecular, o);
    expect(band.strokeCount).toBe(3); // two band outlines, one orphan circle
    o.moleculeStyles.lipid.bondWidthAngstrom = 2;
    expect(renderInteractionPreview(molecular, o).svg).not.toBe(band.svg);
    o.moleculeStyles.lipid.lines = 20;
    expect(renderInteractionPreview(molecular, o).strokeCount).toBe(3); // drag remains outlines-only
    o.moleculeStyles.lipid.bondWidthAngstrom = 0;
    expect(renderInteractionPreview(molecular, o).strokeCount).toBe(2);
    expect(molecular).toEqual(before);
    molecular.molecules![0]!.atoms[1] = [0, 0, 2];
    o.moleculeStyles.lipid.bondWidthAngstrom = 2;
    const endOn = renderInteractionPreview(molecular, o);
    expect(endOn.strokeCount).toBe(2);
    expect(endOn.svg).not.toMatch(/NaN|Infinity/);
    validateSvg(endOn.svg);
  });
  it("handles empty selections without nonfinite SVG and keys only sample/selection geometry", () => {
    const options = defaultRenderOptions();
    const empty = renderInteractionPreview({ ...scene, ribbons: [] }, options);
    expect(empty.strokeCount).toBe(0);
    expect(empty.svg).not.toMatch(/NaN|Infinity/);
    const key = previewGeometryKey(options);
    options.rotation = [5, 10, 15];
    options.lines = 20;
    options.penWidthMm = 2;
    expect(previewGeometryKey(options)).toBe(key);
    options.selectedChains = ["B", "A", "A"];
    const selected = previewGeometryKey(options);
    options.selectedChains = ["A", "B"];
    expect(previewGeometryKey(options)).toBe(selected);
    options.samples = 2;
    expect(previewGeometryKey(options)).not.toBe(selected);
  });
});
