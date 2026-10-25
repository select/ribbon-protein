/// <reference lib="webworker" />
import type { WorkerRequest, WorkerResponse } from "../lib/worker-protocol";
import type { RenderResult, RenderTimings, StructureData } from "../types/render";

interface Session {
  render_options_json(options: string, preview: boolean): string;
  preview_scene_json(): string;
  free(): void;
}
interface RibbonWasm {
  default: (options?: { module_or_path: URL }) => Promise<unknown>;
  RenderSession: new (structure: string) => Session;
}
const booted = performance.now();
let first = true;
let wasm: Promise<RibbonWasm> | undefined;
let session: Session | undefined;
let structure: StructureData | undefined;
let revision: number | undefined;
let parseDsspMs = 0;
let moduleInitializations = 0;
let structureLoads = 0;
let parses = 0;
function loadWasm(url: string): Promise<RibbonWasm> {
  if (!wasm) {
    wasm = (async () => {
      const module = (await import(/* @vite-ignore */ url)) as RibbonWasm;
      moduleInitializations++;
      await module.default({ module_or_path: new URL("ribbon_core_bg.wasm", url) });
      if (typeof module.RenderSession !== "function")
        throw new Error("WASM RenderSession export is missing.");
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
function replace(next: StructureData | undefined, nextRevision: number): void {
  session?.free();
  session = undefined;
  structure = next;
  revision = nextRevision;
}
// The client sends only one active message; async module/parser imports can
// therefore never race another job or create simultaneous synchronous renders.
self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const message = event.data;
  let response: WorkerResponse;
  try {
    if (message.type === "parse") {
      replace(undefined, message.revision);
      const started = performance.now();
      // Keep explicit Vite prebundling in nuxt.config.ts for these lazy imports.
      const { parseStructure } = await import("../lib/parser");
      structure = await parseStructure(message.text, message.name);
      parses++;
      parseDsspMs = performance.now() - started;
      response = { id: message.id, type: "parsed", structure };
    } else {
      if (message.structure) {
        replace(message.structure, message.revision);
        parseDsspMs = 0; // Rehydration never reparses atoms or recomputes DSSP.
      }
      if (revision !== message.revision || !structure)
        throw new Error("Structure revision is not loaded. Retry the calculation.");
      const timings: RenderTimings = { parseDsspMs };
      parseDsspMs = 0;
      const init = performance.now();
      const cold = !wasm;
      const module = await loadWasm(message.wasmUrl);
      timings.moduleInitMs = cold ? performance.now() - init : 0;
      const loaded = performance.now();
      if (!session) {
        session = new module.RenderSession(JSON.stringify(structure));
        structureLoads++;
      }
      timings.structureLoadMs = performance.now() - loaded;
      const started = performance.now();
      const output = JSON.parse(
        session.render_options_json(JSON.stringify(message.options), message.quality === "preview"),
      ) as { result: RenderResult; timings: RenderTimings };
      const result = output.result;
      if (message.includePreviewScene)
        result.previewScene = JSON.parse(session.preview_scene_json());
      result.elapsedMs = performance.now() - started;
      result.quality = message.quality;
      result.timings = {
        ...timings,
        ...output.timings,
        startupMs: first ? started - booted : 0,
        moduleInitializations,
        structureLoads,
        parses,
        structureRevision: revision,
      };
      first = false;
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
      fatal: cause instanceof WebAssembly.RuntimeError,
    };
  }
  self.postMessage(response);
};
