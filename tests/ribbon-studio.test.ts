import { effectScope } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createRibbonStudio } from "../app/lib/ribbon-studio";
import type { RibbonRuntime } from "../app/lib/worker-client";
import type { RenderResult } from "../app/types/render";
import { deferred, result, structure } from "./helpers";

function harness() {
  const runtime = {
    parse: vi.fn().mockResolvedValue(structure),
    render: vi.fn().mockResolvedValue(result),
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
    h.studio.options.widthMm = NaN;
    await vi.advanceTimersByTimeAsync(180);
    expect(h.studio.status.value).toBe("error");
    expect(h.studio.result.value).toEqual(result);
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
  it("fetches only the self-hosted example and ignores late fetch completions", async () => {
    const h = harness();
    await h.studio.loadExample();
    expect(h.fetch.mock.calls[0]![0]).toBe("https://example.test/studio/examples/synthetic.pdb");
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
    await h.studio.loadFile(file("bad.txt"));
    expect(h.studio.error.value).toMatch(/Choose a/);
    expect(h.studio.result.value).toEqual(result);
    h.runtime.parse.mockRejectedValueOnce(new Error("no complete residues"));
    await h.studio.loadFile(file());
    expect(h.studio.error.value).toBe("no complete residues");
    await h.studio.loadFile(file());
    expect(h.studio.status.value).toBe("ready");
    h.dispose();
  });
});
