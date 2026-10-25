import type { RenderRequest, RenderResult, StructureData } from "../types/render";
import type { WorkerRequest, WorkerResponse } from "./worker-protocol";
import { LIMITS } from "./limits";

export interface RibbonRuntime {
  parse(text: string, name: string): Promise<StructureData>;
  render(request: RenderRequest): Promise<RenderResult>;
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

/** One in-flight calculation, hard cancellation and lazy recreation. Generation
 * identity plus job IDs suppress events that arrive after worker termination.
 */
export class RibbonWorkerClient implements RibbonRuntime {
  private worker?: WorkerPort;
  private id = 0;
  private disposed = false;
  private pending?: {
    id: number;
    resolve: (response: WorkerResponse) => void;
    reject: (cause: Error) => void;
    timer: ReturnType<typeof setTimeout>;
  };
  constructor(
    private readonly factory: () => WorkerPort,
    private readonly wasmUrl: string,
  ) {}
  private stop(cause: Error): void {
    const pending = this.pending;
    this.pending = undefined;
    if (pending) {
      clearTimeout(pending.timer);
      pending.reject(cause);
    }
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
  }
  private send(message: WorkerRequest): Promise<WorkerResponse> {
    if (this.disposed) return Promise.reject(new Error("Ribbon runtime has been disposed."));
    this.cancel();
    return new Promise((resolve, reject) => {
      try {
        const worker = this.factory();
        this.worker = worker;
        const timer = setTimeout(
          () =>
            this.stop(
              new Error(
                "Local calculation exceeded two minutes. Reduce quality or input size and try again.",
              ),
            ),
          LIMITS.jobTimeoutMs,
        );
        this.pending = { id: message.id, resolve, reject, timer };
        worker.onmessage = (event) => {
          if (this.worker !== worker || this.pending?.id !== event.data.id) return;
          const pending = this.pending;
          this.pending = undefined;
          clearTimeout(pending.timer);
          if (event.data.type === "error") pending.reject(new Error(event.data.message));
          else pending.resolve(event.data);
        };
        worker.onerror = (event) => {
          event.preventDefault?.();
          if (this.worker === worker)
            this.stop(
              new Error(
                event.message ||
                  "The calculation worker crashed. Try a smaller structure or lower quality.",
              ),
            );
        };
        worker.onmessageerror = () => {
          if (this.worker === worker)
            this.stop(new Error("The calculation worker returned unreadable data. Try again."));
        };
        worker.postMessage(message);
      } catch (cause) {
        this.stop(cause instanceof Error ? cause : new Error(String(cause)));
        reject(cause);
      }
    });
  }
  async parse(text: string, name: string): Promise<StructureData> {
    const response = await this.send({ id: ++this.id, type: "parse", text, name });
    if (response.type !== "parsed") throw new Error("Unexpected parser response.");
    return response.structure;
  }
  async render(request: RenderRequest): Promise<RenderResult> {
    const response = await this.send({
      id: ++this.id,
      type: "render",
      request,
      wasmUrl: this.wasmUrl,
    });
    if (response.type !== "rendered") throw new Error("Unexpected renderer response.");
    return response.result;
  }
}
