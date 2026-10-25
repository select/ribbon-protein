import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkerPort } from "../app/lib/worker-client";
import { RibbonWorkerClient, publicAssetUrl } from "../app/lib/worker-client";
import { defaultRenderOptions } from "../app/types/render";
import type { WorkerRequest, WorkerResponse } from "../app/lib/worker-protocol";
import { LIMITS } from "../app/lib/limits";
import { structure, result } from "./helpers";

function harness() {
  const workers: (WorkerPort & { sent: WorkerRequest[] })[] = [];
  const client = new RibbonWorkerClient(() => {
    const port: WorkerPort & { sent: WorkerRequest[] } = {
      onmessage: null,
      onerror: null,
      onmessageerror: null,
      sent: [],
      postMessage: vi.fn((m: WorkerRequest) => port.sent.push(structuredClone(m))),
      terminate: vi.fn(),
    };
    workers.push(port);
    return port;
  }, "https://example.test/studio/wasm/ribbon_core.js");
  const reply = (
    index = 0,
    response: WorkerResponse = { id: 0, type: "rendered", result: structuredClone(result) },
  ) => {
    const worker = workers[index]!;
    worker.onmessage?.({
      data: { ...response, id: worker.sent.at(-1)!.id },
    } as MessageEvent<WorkerResponse>);
  };
  const render = (lines = 7, quality: "preview" | "final" = "final") =>
    client.render({ structure, options: { ...defaultRenderOptions(), lines } }, quality);
  return { workers, client, reply, render };
}
const aborted = (promise: Promise<unknown>) =>
  expect(promise).rejects.toMatchObject({ name: "AbortError" });
afterEach(() => vi.useRealTimers());
describe("persistent typed worker lifecycle", () => {
  it("reuses the parser's worker and sends only revision/options on warm renders", async () => {
    const h = harness();
    const parsed = h.client.parse("atoms", "test.pdb");
    h.reply(0, { id: 0, type: "parsed", structure });
    const source = await parsed;
    const request = { structure: source, options: defaultRenderOptions() };
    const first = h.client.render(request);
    expect(h.workers[0]!.sent.at(-1)).toMatchObject({
      type: "render",
      revision: 1,
      quality: "final",
    });
    expect(h.workers[0]!.sent.at(-1)).not.toHaveProperty("structure");
    h.reply();
    await first;
    h.client.invalidate();
    const next = h.client.render(request);
    h.reply();
    await next;
    expect(h.workers).toHaveLength(1);
    expect(h.workers[0]!.terminate).not.toHaveBeenCalled();
    h.client.dispose();
  });
  it("transfers the bounded preview scene only for new geometry keys, including obsolete job recovery", async () => {
    const h = harness();
    const scene = {
      center: [0, 0, 0] as [number, number, number],
      basis: [
        [1, 0, 0],
        [0, 1, 0],
        [0, 0, 1],
      ] as [[number, number, number], [number, number, number], [number, number, number]],
      ribbons: [],
    };
    const first = h.render();
    const a = aborted(first);
    const next = h.render(20);
    expect(h.workers[0]!.sent[0]).toMatchObject({ includePreviewScene: true });
    h.reply(0, { id: 0, type: "rendered", result: { ...result, previewScene: scene } });
    await a;
    expect(h.workers[0]!.sent.at(-1)).toMatchObject({ includePreviewScene: false });
    h.reply();
    expect((await next).previewScene).toEqual(scene);
    const changed = h.client.render({
      structure,
      options: { ...defaultRenderOptions(), samples: 2 },
    });
    expect(h.workers[0]!.sent.at(-1)).toMatchObject({ includePreviewScene: true });
    h.reply();
    await changed;
    h.client.dispose();
  });
  it("bounds synchronous active work plus one newest request and settles every superseded promise", async () => {
    const h = harness();
    const first = h.render(1);
    const firstRejected = aborted(first);
    const middle = h.render(2, "preview");
    const middleRejected = aborted(middle);
    const last = h.render(20);
    expect(h.workers).toHaveLength(1);
    expect(h.workers[0]!.sent).toHaveLength(1);
    h.reply();
    await Promise.all([firstRejected, middleRejected]);
    expect(h.workers[0]!.sent).toHaveLength(2);
    expect(h.workers[0]!.sent[1]).toMatchObject({
      type: "render",
      options: { lines: 20 },
      quality: "final",
    });
    expect(h.workers[0]!.sent[1]).not.toHaveProperty("structure");
    h.reply();
    await last;
    h.client.dispose();
  });
  it("invalidates queued/active work without terminating and snapshots mutable options", async () => {
    const h = harness();
    const first = h.render();
    const a = aborted(first);
    const options = defaultRenderOptions();
    const queued = h.client.render({ structure, options });
    const b = aborted(queued);
    options.rotation[0] = 75;
    h.client.invalidate();
    await Promise.all([a, b]);
    h.reply();
    expect(h.workers[0]!.sent).toHaveLength(1);
    const next = h.client.render({ structure, options });
    options.rotation[0] = 90;
    expect(h.workers[0]!.sent.at(-1)).toMatchObject({ options: { rotation: [75, 0, 0] } });
    h.reply();
    await next;
    expect(h.workers[0]!.terminate).not.toHaveBeenCalled();
    h.client.dispose();
  });
  it("hard cancels active and queued work, suppresses late events and rehydrates without parsing", async () => {
    const h = harness();
    const first = h.render();
    const a = aborted(first);
    const queued = h.render(2);
    const b = aborted(queued);
    const staleHandler = h.workers[0]!.onmessage!;
    const staleId = h.workers[0]!.sent[0]!.id;
    h.client.cancel();
    await Promise.all([a, b]);
    expect(h.workers[0]!.terminate).toHaveBeenCalledOnce();
    const next = h.render(3);
    expect(h.workers[1]!.sent[0]).toMatchObject({ type: "render", structure });
    staleHandler({
      data: { id: staleId, type: "rendered", result },
    } as MessageEvent<WorkerResponse>);
    h.reply(1);
    await next;
    h.client.dispose();
    await expect(h.render()).rejects.toThrow(/disposed/);
  });
  it("recovers from crashes, typed errors and unreadable data with generation checks", async () => {
    const h = harness();
    const first = h.render();
    h.workers[0]!.onerror?.({
      message: "worker crashed",
      preventDefault: vi.fn(),
    } as unknown as ErrorEvent);
    await expect(first).rejects.toThrow(/worker crashed/);
    const second = h.render();
    expect(h.workers[1]!.sent[0]).toMatchObject({ structure });
    h.reply(1, { id: 0, type: "error", message: "bad options" });
    await expect(second).rejects.toThrow(/bad options/);
    const third = h.render();
    h.reply(1);
    await third;
    expect(h.workers).toHaveLength(2);
    const fourth = h.render();
    h.workers[1]!.onmessageerror?.({} as MessageEvent);
    await expect(fourth).rejects.toThrow(/unreadable/);
    const fifth = h.render();
    h.reply(2);
    await fifth;
    h.client.dispose();
  });
  it("times out active work (not queue wait), rejects the queue and permits lazy recovery", async () => {
    vi.useFakeTimers();
    const h = harness();
    const first = h.render();
    const a = aborted(first);
    await vi.advanceTimersByTimeAsync(LIMITS.jobTimeoutMs - 10);
    const queued = h.render(2);
    const b = expect(queued).rejects.toThrow(/two minutes/);
    await vi.advanceTimersByTimeAsync(10);
    await Promise.all([a, b]);
    expect(h.workers[0]!.terminate).toHaveBeenCalledOnce();
    const next = h.render();
    h.reply(1);
    await next;
    h.client.dispose();
  });
  it("supersedes structures and parses safely, but retains an idle module on file replacement", async () => {
    const h = harness();
    const old = h.render();
    const a = aborted(old);
    const parsed = h.client.parse("new atoms", "new.pdb");
    await a;
    h.reply(1, { id: 0, type: "parsed", structure });
    const source = await parsed;
    const next = h.client.render({ structure: source, options: defaultRenderOptions() });
    h.reply(1);
    await next;
    const replacement = h.client.parse("more atoms", "more.pdb");
    expect(h.workers).toHaveLength(2);
    h.reply(1, { id: 0, type: "parsed", structure });
    await replacement;
    h.client.dispose();
  });
  it("gives the newest job its own full active timeout after a long queue wait", async () => {
    vi.useFakeTimers();
    const h = harness();
    const first = h.render();
    const a = aborted(first);
    const next = h.render(20);
    await vi.advanceTimersByTimeAsync(LIMITS.jobTimeoutMs - 10);
    h.reply();
    await a;
    await vi.advanceTimersByTimeAsync(20);
    expect(h.workers[0]!.terminate).not.toHaveBeenCalled();
    h.reply();
    await next;
    h.client.dispose();
  });
  it("drops poisoned wasm instances and rehydrates after a fatal trap", async () => {
    const h = harness();
    const first = h.render();
    h.reply(0, { id: 0, type: "error", message: "wasm trap", fatal: true });
    await expect(first).rejects.toThrow(/wasm trap/);
    expect(h.workers[0]!.terminate).toHaveBeenCalledOnce();
    const next = h.render();
    expect(h.workers[1]!.sent[0]).toMatchObject({ structure });
    h.reply(1);
    await next;
    h.client.dispose();
  });
  it("interrupts a previous parse and a cross-structure render, never mixing revisions", async () => {
    const h = harness();
    const old = h.client.parse("old", "old.pdb");
    const a = aborted(old);
    const next = h.client.parse("new", "new.pdb");
    await a;
    h.reply(1, { id: 0, type: "parsed", structure });
    await next;
    const active = h.render();
    const b = aborted(active);
    const replacement = { ...structure, name: "other molecule", segments: [] };
    const rendered = h.client.render({ structure: replacement, options: defaultRenderOptions() });
    await b;
    expect(h.workers[2]!.sent[0]).toMatchObject({ revision: 3, structure: replacement });
    h.reply(2);
    await rendered;
    h.client.dispose();
  });
  it("rejects synchronous worker construction/posting failures and can retry", async () => {
    const factory = vi.fn<() => WorkerPort>().mockImplementationOnce(() => {
      throw new Error("construction failed");
    });
    const client = new RibbonWorkerClient(factory, "https://example.test/wasm.js");
    await expect(client.render({ structure, options: defaultRenderOptions() })).rejects.toThrow(
      /construction failed/,
    );
    const worker: WorkerPort = {
      onmessage: null,
      onerror: null,
      onmessageerror: null,
      postMessage: vi.fn(() => {
        throw new Error("clone failed");
      }),
      terminate: vi.fn(),
    };
    factory.mockReturnValue(worker);
    await expect(client.render({ structure, options: defaultRenderOptions() })).rejects.toThrow(
      /clone failed/,
    );
    expect(worker.terminate).toHaveBeenCalledOnce();
    client.dispose();
  });
  it("disposes active and queued requests without leaks", async () => {
    const h = harness();
    const first = h.render();
    const a = aborted(first);
    const next = h.render();
    const b = aborted(next);
    h.client.dispose();
    await Promise.all([a, b]);
    expect(h.workers[0]!.onmessage).toBeNull();
    await expect(h.client.parse("no", "no.pdb")).rejects.toThrow(/disposed/);
  });
  it("resolves assets at Nuxt baseURL, not a nested route", () => {
    expect(
      publicAssetUrl("/ribbon/", "/wasm/ribbon_core.js", "https://host.test/nested/page"),
    ).toBe("https://host.test/ribbon/wasm/ribbon_core.js");
    expect(publicAssetUrl("/", "examples/synthetic.pdb", "https://host.test/a")).toBe(
      "https://host.test/examples/synthetic.pdb",
    );
  });
});
