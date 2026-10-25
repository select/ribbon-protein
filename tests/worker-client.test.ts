import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkerPort } from "../app/lib/worker-client";
import { RibbonWorkerClient, publicAssetUrl } from "../app/lib/worker-client";
import { defaultRenderOptions } from "../app/types/render";
import type { WorkerRequest, WorkerResponse } from "../app/lib/worker-protocol";
import { LIMITS } from "../app/lib/limits";
import { structure, result } from "./helpers";

function harness() {
  const workers: (WorkerPort & { sent?: WorkerRequest })[] = [];
  const client = new RibbonWorkerClient(() => {
    const port: WorkerPort & { sent?: WorkerRequest } = {
      onmessage: null,
      onerror: null,
      onmessageerror: null,
      postMessage: vi.fn((m: WorkerRequest) => {
        port.sent = m;
      }),
      terminate: vi.fn(),
    };
    workers.push(port);
    return port;
  }, "https://example.test/studio/wasm/ribbon_core.js");
  const reply = (index: number, message: Omit<WorkerResponse, "id">) => {
    const worker = workers[index]!;
    worker.onmessage?.({
      data: { id: worker.sent!.id, ...message },
    } as MessageEvent<WorkerResponse>);
  };
  return { workers, client, reply };
}
afterEach(() => vi.useRealTimers());
describe("typed worker lifecycle", () => {
  it("terminates on cancel, recreates and suppresses late results", async () => {
    const h = harness();
    const old = h.client.parse("old", "old.pdb");
    const rejection = expect(old).rejects.toMatchObject({ name: "AbortError" });
    const staleHandler = h.workers[0]!.onmessage!;
    const staleId = h.workers[0]!.sent!.id;
    h.client.cancel();
    await rejection;
    expect(h.workers[0]!.terminate).toHaveBeenCalledOnce();
    const next = h.client.parse("next", "next.pdb");
    staleHandler({
      data: { id: staleId, type: "parsed", structure: { ...structure, name: "old" } },
    } as MessageEvent<WorkerResponse>);
    h.reply(1, { type: "parsed", structure } as Omit<WorkerResponse, "id">);
    expect(await next).toEqual(structure);
    h.client.dispose();
    await expect(h.client.parse("no", "no.pdb")).rejects.toThrow(/disposed/);
  });
  it("recovers from worker crashes and typed errors", async () => {
    const h = harness();
    const first = h.client.parse("", "bad.pdb");
    h.workers[0]!.onerror?.({
      message: "worker crashed",
      preventDefault: vi.fn(),
    } as unknown as ErrorEvent);
    await expect(first).rejects.toThrow(/worker crashed/);
    const second = h.client.parse("", "bad.pdb");
    h.reply(1, { type: "error", message: "no atoms" } as Omit<WorkerResponse, "id">);
    await expect(second).rejects.toThrow(/no atoms/);
    const third = h.client.render({ structure, options: defaultRenderOptions() });
    expect(h.workers[2]!.sent).toMatchObject({
      type: "render",
      wasmUrl: "https://example.test/studio/wasm/ribbon_core.js",
    });
    h.reply(2, { type: "rendered", result } as Omit<WorkerResponse, "id">);
    expect(await third).toEqual(result);
    h.client.dispose();
  });
  it("times out stalled workers and rejects message decoding errors", async () => {
    vi.useFakeTimers();
    const h = harness();
    const job = h.client.parse("", "stall.pdb");
    const rejected = expect(job).rejects.toThrow(/two minutes/);
    await vi.advanceTimersByTimeAsync(LIMITS.jobTimeoutMs);
    await rejected;
    const next = h.client.parse("", "bad.pdb");
    h.workers[1]!.onmessageerror?.({} as MessageEvent);
    await expect(next).rejects.toThrow(/unreadable/);
    h.client.dispose();
  });
  it("resolves public assets at Nuxt baseURL, never at a nested route", () => {
    expect(
      publicAssetUrl("/ribbon/", "/wasm/ribbon_core.js", "https://host.test/nested/page"),
    ).toBe("https://host.test/ribbon/wasm/ribbon_core.js");
    expect(publicAssetUrl("/", "examples/synthetic.pdb", "https://host.test/a")).toBe(
      "https://host.test/examples/synthetic.pdb",
    );
  });
});
