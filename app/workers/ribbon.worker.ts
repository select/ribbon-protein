/// <reference lib="webworker" />
import { parseStructure } from "../lib/parser";
import type { WorkerRequest, WorkerResponse } from "../lib/worker-protocol";
import type { RenderResult } from "../types/render";

interface RibbonWasm {
  default: (options?: { module_or_path: URL }) => Promise<unknown>;
  render_json: (request: string) => string;
}
let wasm: Promise<RibbonWasm> | undefined;
function loadWasm(url: string): Promise<RibbonWasm> {
  if (!wasm) {
    wasm = (async () => {
      const module = (await import(/* @vite-ignore */ url)) as RibbonWasm;
      await module.default({ module_or_path: new URL("ribbon_core_bg.wasm", url) });
      if (typeof module.render_json !== "function")
        throw new Error("WASM render_json export is missing.");
      return module;
    })().catch((cause) => {
      wasm = undefined;
      throw new Error(
        `Unable to load the local WASM renderer. Run pnpm build:wasm and reload. ${cause instanceof Error ? cause.message : String(cause)}`,
      );
    });
  }
  return wasm;
}
self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const message = event.data;
  let response: WorkerResponse;
  try {
    if (message.type === "parse") {
      response = {
        id: message.id,
        type: "parsed",
        structure: await parseStructure(message.text, message.name),
      };
    } else {
      const module = await loadWasm(message.wasmUrl);
      const started = performance.now();
      const result = JSON.parse(
        module.render_json(JSON.stringify(message.request)),
      ) as RenderResult;
      result.elapsedMs = performance.now() - started;
      if (
        typeof result.svg !== "string" ||
        !result.svg.includes("<svg") ||
        !Number.isFinite(result.lengthMm) ||
        !Number.isFinite(result.strokeCount)
      )
        throw new Error("The WASM renderer returned an invalid result.");
      response = { id: message.id, type: "rendered", result };
    }
  } catch (cause) {
    response = {
      id: message.id,
      type: "error",
      message: cause instanceof Error ? cause.message : String(cause),
    };
  }
  self.postMessage(response);
};
