import type {
  PreviewScene,
  RenderOptions,
  RenderRequest,
  RenderResult,
  StructureData,
} from "../types/render";
import { previewGeometryKey } from "./interaction-preview";
import type { WorkerRequest, WorkerResponse } from "./worker-protocol";
import { LIMITS } from "./limits";

export interface RibbonRuntime {
  parse(text: string, name: string): Promise<StructureData>;
  render(request: RenderRequest, quality?: "preview" | "final"): Promise<RenderResult>;
  /** Reject obsolete work without destroying a useful worker/module/cache. */
  invalidate(): void;
  /** Hard interrupt, including synchronous WASM. Rehydrate lazily next time. */
  cancel(): void;
  dispose(): void;
}
export interface WorkerPort {
  onmessage: ((event: MessageEvent<WorkerResponse>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  onmessageerror: ((event: MessageEvent) => void) | null;
  postMessage(message: WorkerRequest): void;
  terminate(): void;
}
export function abortError(): Error {
  return new DOMException("Calculation cancelled.", "AbortError");
}
export function isAbort(cause: unknown): boolean {
  return cause instanceof Error && cause.name === "AbortError";
}

/** baseURL may be / or /a/subpath/. Never resolve assets relative to the current route. */
export function publicAssetUrl(baseURL: string, path: string, origin: string): string {
  const base = new URL(baseURL.endsWith("/") ? baseURL : `${baseURL}/`, origin);
  return new URL(path.replace(/^\/+/, ""), base).href;
}
interface Job {
  message: WorkerRequest;
  resolve: (response: WorkerResponse) => void;
  reject: (cause: Error) => void;
  settled: boolean;
  timer?: ReturnType<typeof setTimeout>;
  started?: number;
}

/** Exactly one active message and one replaceable newest render. Worker identity
 * and IDs guard every event. Queue wait never consumes the active-job timeout.
 */
export class RibbonWorkerClient implements RibbonRuntime {
  private worker?: WorkerPort;
  private id = 0;
  private revision = 0;
  private source?: StructureData;
  private loadedRevision?: number;
  private scene?: { revision: number; key: string; data: PreviewScene };
  private disposed = false;
  private active?: Job;
  private newest?: Job;
  constructor(
    private readonly factory: () => WorkerPort,
    private readonly wasmUrl: string,
  ) {}
  private reject(job: Job | undefined, cause: Error): void {
    if (!job || job.settled) return;
    job.settled = true;
    job.reject(cause);
  }
  invalidate(): void {
    this.reject(this.newest, abortError());
    this.newest = undefined;
    this.reject(this.active, abortError());
  }
  private stop(cause: Error): void {
    this.reject(this.active, cause);
    this.reject(this.newest, cause);
    if (this.active?.timer) clearTimeout(this.active.timer);
    this.active = this.newest = undefined;
    this.loadedRevision = undefined;
    if (this.worker) {
      this.worker.onmessage = null;
      this.worker.onerror = null;
      this.worker.onmessageerror = null;
      this.worker.terminate();
      this.worker = undefined;
    }
  }
  cancel(): void {
    this.stop(abortError());
  }
  dispose(): void {
    this.disposed = true;
    this.cancel();
    this.source = undefined;
    this.scene = undefined;
  }
  private port(): WorkerPort {
    if (this.worker) return this.worker;
    const worker = this.factory();
    this.worker = worker;
    worker.onmessage = (event) => {
      const job = this.active;
      if (this.worker !== worker || !job || job.message.id !== event.data.id) return;
      const response = event.data;
      if (response.type === "error" && response.fatal) {
        this.stop(new Error(response.message));
        return;
      }
      clearTimeout(job.timer);
      this.active = undefined;
      if (response.type === "parsed" || response.type === "rendered")
        this.loadedRevision = job.message.revision;
      if (response.type === "rendered" && job.message.type === "render") {
        const key = previewGeometryKey(job.message.options);
        if (response.result.previewScene)
          this.scene = { revision: job.message.revision, key, data: response.result.previewScene };
        else if (this.scene?.revision === job.message.revision && this.scene.key === key)
          response.result.previewScene = this.scene.data;
      }
      if (!job.settled) {
        job.settled = true;
        if (response.type === "error") job.reject(new Error(response.message));
        else {
          if (response.type === "rendered") {
            response.result.timings = {
              ...response.result.timings,
              roundtripMs: performance.now() - job.started!,
            };
          }
          job.resolve(response);
        }
      }
      const next = this.newest;
      this.newest = undefined;
      if (next) this.start(next);
    };
    worker.onerror = (event) => {
      event.preventDefault?.();
      if (this.worker === worker)
        this.stop(new Error(event.message || "The calculation worker crashed. Try again."));
    };
    worker.onmessageerror = () => {
      if (this.worker === worker)
        this.stop(new Error("The calculation worker returned unreadable data. Try again."));
    };
    return worker;
  }
  private start(job: Job): void {
    this.active = job;
    job.started = performance.now();
    job.timer = setTimeout(
      () =>
        this.stop(
          new Error(
            "Local calculation exceeded two minutes. Reduce quality or input size and try again.",
          ),
        ),
      LIMITS.jobTimeoutMs,
    );
    try {
      const worker = this.port();
      const message = job.message;
      if (message.type === "render")
        message.includePreviewScene =
          this.scene?.revision !== message.revision ||
          this.scene.key !== previewGeometryKey(message.options);
      // Normal parse already left normalized coordinates in this same worker.
      // Only a fresh/crashed/cancelled worker needs the browser's retained copy.
      worker.postMessage(
        message.type === "render" && this.loadedRevision !== message.revision
          ? { ...message, structure: this.source }
          : message,
      );
    } catch (cause) {
      this.stop(cause instanceof Error ? cause : new Error(String(cause)));
    }
  }
  private send(message: WorkerRequest): Promise<WorkerResponse> {
    if (this.disposed) return Promise.reject(new Error("Ribbon runtime has been disposed."));
    return new Promise((resolve, reject) => {
      const job: Job = { message, resolve, reject, settled: false };
      if (this.active) {
        this.invalidate();
        this.newest = job;
      } else this.start(job);
    });
  }
  async parse(text: string, name: string): Promise<StructureData> {
    if (this.disposed) throw new Error("Ribbon runtime has been disposed.");
    // A superseding structure may interrupt sync render or asynchronous parsing.
    // Idle replacement retains the module, but the worker drops the old session.
    if (this.active) this.cancel();
    const revision = ++this.revision;
    this.scene = undefined;
    const response = await this.send({ id: ++this.id, revision, type: "parse", text, name });
    if (response.type !== "parsed") throw new Error("Unexpected parser response.");
    this.source = response.structure;
    return response.structure;
  }
  async render(
    request: RenderRequest,
    quality: "preview" | "final" = "final",
  ): Promise<RenderResult> {
    if (this.disposed) throw new Error("Ribbon runtime has been disposed.");
    if (this.source !== request.structure) {
      if (this.active) this.cancel();
      this.source = request.structure;
      this.scene = undefined;
      this.revision++;
    }
    const response = await this.send({
      id: ++this.id,
      revision: this.revision,
      type: "render",
      options: JSON.parse(JSON.stringify(request.options)) as RenderOptions,
      quality,
      wasmUrl: this.wasmUrl,
    });
    if (response.type !== "rendered") throw new Error("Unexpected renderer response.");
    return response.result;
  }
}
