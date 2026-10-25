import { afterEach, describe, expect, it, vi } from "vitest";
import {
  exportFileName,
  pngSize,
  pngBlob,
  saveBlob,
  svgBlob,
  validateSvg,
} from "../app/lib/exports";
import { result } from "./helpers";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
describe("line-only SVG and PNG export helpers", () => {
  it("releases download URLs and removes its temporary anchor", async () => {
    vi.useFakeTimers();
    const create = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:test");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const anchor = { click: vi.fn(), remove: vi.fn(), style: {}, href: "", download: "" };
    const append = vi.fn();
    vi.stubGlobal("document", { createElement: () => anchor, body: { append } });
    saveBlob(new Blob(["svg"]), "protein.svg");
    expect(create).toHaveBeenCalledOnce();
    expect(append).toHaveBeenCalledWith(anchor);
    expect(anchor.download).toBe("protein.svg");
    expect(anchor.click).toHaveBeenCalledOnce();
    expect(anchor.remove).toHaveBeenCalledOnce();
    expect(revoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    expect(revoke).toHaveBeenCalledWith("blob:test");
  });
  it("releases the rasterization URL even when image decoding fails", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:failed");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    vi.stubGlobal(
      "Image",
      class {
        src = "";
        decode() {
          return Promise.reject(new Error("decode failed"));
        }
      },
    );
    await expect(pngBlob(result)).rejects.toThrow(/decode failed/);
    expect(revoke).toHaveBeenCalledWith("blob:failed");
  });
  it("rejects interaction previews at both export boundaries", async () => {
    const preview = { ...result, quality: "preview" as const };
    expect(() => svgBlob(preview)).toThrow(/cannot be exported/);
    await expect(pngBlob(preview)).rejects.toThrow(/cannot be exported/);
  });
  it("preserves actual SVG bytes, physical dimensions and safe filenames", async () => {
    const blob = svgBlob(result);
    expect(blob.type).toBe("image/svg+xml;charset=utf-8");
    expect(await blob.text()).toBe(result.svg);
    expect(exportFileName("../../my protein.cif", "svg")).toBe("my-protein.svg");
    expect(exportFileName("", "png")).toBe("ribbon.png");
  });
  it.each([
    '<svg fill="none"><image href="https://x"/></svg>',
    '<svg fill="none"><path fill="red"/></svg>',
    '<svg fill="none"><mask/></svg>',
    '<svg fill="none"><path onclick="evil()"/></svg>',
    '<svg fill="none"><path stroke="url(https://evil)"/></svg>',
    '<svg fill="none"><path style="fill:red"/></svg>',
    '<!DOCTYPE svg><svg fill="none"/>',
  ])("rejects non-line or active content: %s", (svg) => {
    expect(() => validateSvg(svg)).toThrow();
  });
  it("uses bounded raster dimensions in pixels without changing SVG millimeters", () => {
    expect(pngSize(25.4, 50.8)).toEqual({ width: 150, height: 300 });
    expect(() => pngSize(2000, 2000)).toThrow(/too large/);
    expect(() => pngSize(NaN, 30)).toThrow(/finite/);
  });
});
