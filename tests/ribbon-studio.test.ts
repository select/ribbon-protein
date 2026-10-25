import { effectScope } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRibbonStudio } from "../app/lib/ribbon-studio";
import type { RibbonRuntime } from "../app/lib/worker-client";
import type { RenderResult } from "../app/types/render";
import { deferred, result, structure } from "./helpers";

function harness() {
  const runtime = {
    parse: vi.fn().mockResolvedValue(structure),
    render: vi.fn().mockImplementation(async () => structuredClone(result)),
    invalidate: vi.fn(),
    cancel: vi.fn(),
    dispose: vi.fn(),
  } satisfies RibbonRuntime;
  const save = vi.fn();
  const scope = effectScope();
  const png = vi.fn().mockResolvedValue(new Blob(["png"]));
  const fetch = vi.fn().mockResolvedValue(new Response("example"));
  const studio = scope.run(() =>
    createRibbonStudio(runtime, {
      baseURL: "/studio/",
      origin: "https://example.test",
      save,
      png,
      fetch,
      debounceMs: 180,
    }),
  )!;
  const dispose = () => {
    studio.dispose();
    scope.stop();
  };
  return { studio, runtime, save, png, fetch, dispose };
}
const file = (name = "test.pdb", text: Promise<string> = Promise.resolve("text")) =>
  ({ name, size: 4, text: () => text }) as File;
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("useRibbon reactive runtime", () => {
  it("loads all chains explicitly, debounces direct edits and marks exports stale immediately", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    expect(h.studio.options.selectedChains).toEqual(["A"]);
    expect(h.studio.status.value).toBe("ready");
    expect(h.studio.isStale.value).toBe(false);
    h.studio.options.lines = 8;
    h.studio.options.lines = 9;
    expect(h.studio.isStale.value).toBe(true);
    expect(h.runtime.render).toHaveBeenCalledOnce();
    h.studio.downloadSvg();
    expect(h.save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(180);
    expect(h.runtime.render).toHaveBeenCalledTimes(2);
    expect(h.runtime.render.mock.calls[1]![0].options.lines).toBe(9);
    h.studio.downloadSvg();
    expect(h.save).toHaveBeenCalledOnce();
    h.dispose();
  });
  it("keeps the last preview on invalid options and treats empty selection as none", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    const previous = h.studio.result.value;
    h.studio.options.widthMm = NaN;
    await vi.advanceTimersByTimeAsync(180);
    expect(h.studio.status.value).toBe("error");
    expect(h.studio.result.value).toBe(previous);
    expect(h.studio.error.value).toMatch(/Width/);
    expect(h.studio.isStale.value).toBe(true);
    h.studio.options.widthMm = 210;
    h.runtime.render.mockResolvedValueOnce({
      ...result,
      strokeCount: 0,
      lengthMm: 0,
      residueCount: 0,
    });
    h.studio.options.selectedChains = [];
    await vi.advanceTimersByTimeAsync(180);
    expect(h.runtime.render.mock.calls.at(-1)![0].options.selectedChains).toEqual([]);
    expect(h.studio.result.value?.strokeCount).toBe(0);
    expect(h.studio.status.value).toBe("ready");
    h.dispose();
  });
  it("suppresses obsolete renders and old File.text completions after a newer file or cancel", async () => {
    const h = harness();
    const oldText = deferred<string>();
    const oldLoad = h.studio.loadFile(file("old.pdb", oldText.promise));
    await h.studio.loadFile(file("new.pdb"));
    oldText.resolve("old coordinates");
    await oldLoad;
    expect(h.studio.fileName.value).toBe("new.pdb");
    expect(h.runtime.parse).toHaveBeenCalledOnce();
    const late = deferred<RenderResult>();
    h.runtime.render.mockReturnValueOnce(late.promise);
    const render = h.studio.render();
    h.studio.options.rotation[0] = 45;
    late.resolve({ ...result, strokeCount: 999 });
    await render;
    expect(h.studio.result.value?.strokeCount).toBe(1);
    h.studio.cancel();
    await vi.advanceTimersByTimeAsync(1000);
    expect(h.studio.isStale.value).toBe(true);
    expect(h.studio.status.value).toBe("idle");
    h.dispose();
  });
  it("ignores a cancelled parser completion and lets options edited during loading reach the first render", async () => {
    const h = harness();
    const parsed = deferred<typeof structure>();
    h.runtime.parse.mockReturnValueOnce(parsed.promise);
    const old = h.studio.loadFile(file("old.pdb"));
    await Promise.resolve();
    h.studio.cancel();
    parsed.resolve({ ...structure, name: "old.pdb" });
    await old;
    expect(h.studio.structure.value).toBeNull();
    const nextParsed = deferred<typeof structure>();
    h.runtime.parse.mockReturnValueOnce(nextParsed.promise);
    const next = h.studio.loadFile(file("new.pdb"));
    await Promise.resolve();
    h.studio.options.lines = 12;
    nextParsed.resolve(structure);
    await next;
    expect(h.runtime.render.mock.calls.at(-1)![0].options.lines).toBe(12);
    expect(h.studio.isStale.value).toBe(false);
    h.dispose();
  });
  it("maps preset dimensions centrally and preserves custom dimensions on orientation changes", () => {
    const h = harness();
    h.studio.options.paper = "a3";
    expect([h.studio.options.widthMm, h.studio.options.heightMm]).toEqual([297, 420]);
    h.studio.options.landscape = true;
    expect([h.studio.options.widthMm, h.studio.options.heightMm]).toEqual([420, 297]);
    h.studio.options.paper = "bierdeckel";
    expect(h.studio.options.marginMm).toBe(5);
    expect(h.studio.options.widthMm).toBe(93.14);
    h.studio.options.paper = "custom";
    h.studio.options.widthMm = 125;
    h.studio.options.heightMm = 200;
    h.studio.options.landscape = false;
    expect([h.studio.options.widthMm, h.studio.options.heightMm]).toEqual([125, 200]);
    h.studio.options.paper = "a4";
    h.studio.options.paper = "custom";
    expect([h.studio.options.widthMm, h.studio.options.heightMm]).toEqual([125, 200]);
    h.dispose();
  });
  it("suppresses a PNG which becomes obsolete during rasterization and disposes timers", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    const encoded = deferred<Blob>();
    h.png.mockReturnValueOnce(encoded.promise);
    const download = h.studio.downloadPng();
    h.studio.options.penWidthMm = 0.3;
    encoded.resolve(new Blob(["png"]));
    await download;
    expect(h.save).not.toHaveBeenCalled();
    h.dispose();
    await vi.advanceTimersByTimeAsync(1000);
    expect(h.runtime.render).toHaveBeenCalledOnce();
    expect(h.runtime.dispose).toHaveBeenCalledOnce();
  });
  it("keeps interaction previews display-only and refines exact options immediately on release", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    const exact = h.studio.result.value;
    h.studio.beginInteraction();
    h.studio.options.rotation = [10, 20, 0];
    expect(h.studio.isStale.value).toBe(true);
    await vi.advanceTimersByTimeAsync(24);
    expect(h.runtime.render.mock.calls.at(-1)?.[1]).toBe("preview");
    expect(h.studio.previewResult.value?.quality).toBe("preview");
    expect(h.studio.result.value).toBe(exact);
    h.studio.downloadSvg();
    await h.studio.downloadPng();
    expect(h.save).not.toHaveBeenCalled();
    const final = deferred<RenderResult>();
    h.runtime.render.mockReturnValueOnce(final.promise);
    h.studio.endInteraction();
    expect(h.runtime.render.mock.calls.at(-1)?.[1]).toBe("final");
    expect(h.studio.status.value).toBe("rendering");
    expect(h.studio.isStale.value).toBe(true);
    final.resolve({ ...result, strokeCount: 456 });
    await Promise.resolve();
    await Promise.resolve();
    expect(h.studio.previewResult.value).toBeNull();
    expect(h.studio.result.value?.strokeCount).toBe(456);
    expect(h.studio.isStale.value).toBe(false);
    expect(h.runtime.cancel).not.toHaveBeenCalled();
    h.dispose();
  });
  it("uses bounded prepared 3D contours locally during dragging, without starving behind active wasm", async () => {
    const h = harness();
    const scene = {
      center: [0, 0, 0] as [number, number, number],
      basis: [
        [1, 0, 0],
        [0, 1, 0],
        [0, 0, 1],
      ] as [[number, number, number], [number, number, number], [number, number, number]],
      ribbons: [
        {
          chainId: "A",
          samples: structure.segments[0]!.residues.map((r) => ({
            point: r.ca,
            side: r.carbonyl,
            width: 0.2,
            alpha: 1,
          })),
        },
      ],
    };
    h.runtime.render.mockResolvedValueOnce({ ...result, previewScene: scene });
    await h.studio.loadFile(file());
    const exact = h.studio.result.value;
    const active = deferred<RenderResult>();
    h.runtime.render.mockReturnValueOnce(active.promise);
    const old = h.studio.render();
    h.studio.beginInteraction();
    // Faster than a frame: trailing debounce would starve every preview here.
    for (let i = 1; i <= 40; i++) {
      h.studio.options.rotation = [i, 2 * i, 0];
      await vi.advanceTimersByTimeAsync(4);
      if (i >= 4) expect(h.studio.previewResult.value?.quality).toBe("preview");
    }
    expect(h.runtime.render).toHaveBeenCalledTimes(2); // initial + obsolete active full only
    expect(h.studio.result.value).toBe(exact);
    h.studio.endInteraction();
    await Promise.resolve();
    await Promise.resolve();
    active.resolve({ ...result, strokeCount: 999 });
    await old;
    expect(h.studio.result.value?.strokeCount).toBe(1);
    expect(h.studio.previewResult.value).toBeNull();
    h.studio.beginInteraction();
    h.studio.options.samples = 2;
    await vi.advanceTimersByTimeAsync(24);
    expect(h.runtime.render.mock.calls.at(-1)?.[1]).toBe("preview"); // new scene key must be prepared
    h.dispose();
  });
  it("never lets a late preview overwrite a newer final or a replacement structure", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    h.studio.beginInteraction();
    const preview = deferred<RenderResult>();
    h.runtime.render.mockReturnValueOnce(preview.promise);
    h.studio.options.rotation = [10, 20, 30];
    await vi.advanceTimersByTimeAsync(24);
    h.studio.endInteraction();
    await Promise.resolve();
    await Promise.resolve();
    const exact = h.studio.result.value;
    preview.resolve({ ...result, strokeCount: 999 });
    await Promise.resolve();
    await Promise.resolve();
    expect(h.studio.result.value).toBe(exact);
    expect(h.studio.previewResult.value).toBeNull();
    h.studio.beginInteraction();
    const old = deferred<RenderResult>();
    h.runtime.render.mockReturnValueOnce(old.promise);
    h.studio.options.rotation = [30, 40, 50];
    await vi.advanceTimersByTimeAsync(24);
    await h.studio.loadFile(file("replacement.pdb"));
    old.resolve({ ...result, strokeCount: 1000 });
    await Promise.resolve();
    await Promise.resolve();
    expect(h.studio.fileName.value).toBe("replacement.pdb");
    expect(h.studio.result.value?.strokeCount).toBe(1);
    expect(h.studio.interacting.value).toBe(false);
    h.dispose();
  });
  it("hard cancellation and reset settle interactions without exporting previews", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    h.studio.beginInteraction();
    h.studio.options.rotation = [40, 20, 0];
    h.studio.cancel();
    expect(h.runtime.cancel).toHaveBeenCalledOnce();
    expect(h.studio.interacting.value).toBe(false);
    await vi.advanceTimersByTimeAsync(500);
    expect(h.runtime.render).toHaveBeenCalledOnce();
    h.studio.resetView();
    await vi.advanceTimersByTimeAsync(180);
    expect(h.runtime.render.mock.calls.at(-1)?.[1]).toBe("final");
    expect(h.studio.options.rotation).toEqual([0, 0, 0]);
    h.dispose();
  });
  it("records end-to-end timing only for the currently displayed result", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    const old = h.studio.result.value!;
    h.studio.markDisplayed(old);
    expect(old.timings?.visibleMs).toBeGreaterThanOrEqual(0);
    h.studio.options.rotation = [5, 10, 15];
    await vi.advanceTimersByTimeAsync(180);
    const next = h.studio.result.value!;
    h.studio.markDisplayed(old);
    expect(next.timings?.visibleMs).toBeUndefined();
    h.studio.markDisplayed(next);
    expect(next.timings?.visibleMs).toBeGreaterThanOrEqual(next.timings!.responseMs!);
    h.dispose();
  });
  it("fetches only the self-hosted example and ignores late fetch completions", async () => {
    const h = harness();
    await h.studio.loadExample();
    expect(h.fetch.mock.calls[0]![0]).toBe(
      "https://example.test/studio/examples/1crn-backbone.pdb",
    );
    const response = deferred<Response>();
    h.fetch.mockReturnValueOnce(response.promise);
    const load = h.studio.loadExample();
    h.studio.cancel();
    response.resolve(new Response("late"));
    await load;
    expect(h.runtime.parse).toHaveBeenCalledOnce();
    expect(h.studio.isStale.value).toBe(true);
    h.dispose();
  });
  it("reports invalid files/recoverable worker failures without replacing valid previews", async () => {
    const h = harness();
    await h.studio.loadFile(file());
    const previous = h.studio.result.value;
    await h.studio.loadFile(file("bad.txt"));
    expect(h.studio.error.value).toMatch(/Choose a/);
    expect(h.studio.result.value).toBe(previous);
    h.runtime.parse.mockRejectedValueOnce(new Error("no complete residues"));
    await h.studio.loadFile(file());
    expect(h.studio.error.value).toBe("no complete residues");
    await h.studio.loadFile(file());
    expect(h.studio.status.value).toBe("ready");
    h.dispose();
  });
});
