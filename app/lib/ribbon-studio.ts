import { reactive, ref, shallowRef, computed, watch } from "vue";
import { defaultRenderOptions, PAPER_PRESETS } from "../types/render";
import type { RenderOptions, StructureData, RenderResult, RenderStatus } from "../types/render";
import { isAbort, publicAssetUrl } from "./worker-client";
import type { RibbonRuntime } from "./worker-client";
import { validateFile, LIMITS } from "./limits";
import { validateOptions } from "./options";
import { exportFileName, pngBlob, saveBlob, svgBlob } from "./exports";

interface StudioEnvironment {
  baseURL: string;
  origin: string;
  fetch?: typeof fetch;
  save?: typeof saveBlob;
  png?: typeof pngBlob;
  debounceMs?: number;
}
/** Framework-independent Vue state for deterministic lifecycle/race tests.
 * useRibbon supplies browser/Nuxt dependencies and scope disposal.
 */
export function createRibbonStudio(runtime: RibbonRuntime, env: StudioEnvironment) {
  const options = reactive<RenderOptions>(defaultRenderOptions());
  const structure = shallowRef<StructureData | null>(null);
  const result = shallowRef<RenderResult | null>(null);
  const status = ref<RenderStatus>("idle");
  const error = ref<string | null>(null);
  const fileName = ref("");
  let epoch = 0,
    disposed = false,
    quiet = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let fetchAbort: AbortController | undefined;
  const revision = ref(0);
  const renderedRevision = ref(-1);
  const isStale = computed(
    () => !result.value || revision.value !== renderedRevision.value || status.value !== "ready",
  );
  const stopTimer = () => {
    if (timer) clearTimeout(timer);
    timer = undefined;
  };
  const fail = (cause: unknown) => {
    error.value = cause instanceof Error ? cause.message : String(cause);
    status.value = "error";
  };
  const invalidate = () => {
    stopTimer();
    epoch++;
    runtime.cancel();
    revision.value++;
  };
  const render = async (): Promise<void> => {
    if (disposed || status.value === "loading") return;
    stopTimer();
    const source = structure.value;
    if (!source) return;
    invalidate();
    const job = epoch,
      requestedRevision = revision.value;
    try {
      const snapshot = JSON.parse(JSON.stringify(options)) as RenderOptions;
      validateOptions(snapshot, source);
      error.value = null;
      status.value = "rendering";
      const next = await runtime.render({ structure: source, options: snapshot });
      if (disposed || epoch !== job) return;
      result.value = next;
      renderedRevision.value = requestedRevision;
      status.value = "ready";
    } catch (cause) {
      if (disposed || epoch !== job || isAbort(cause)) return;
      fail(cause);
    }
  };
  let customPaper: { widthMm: number; heightMm: number; marginMm: number } | undefined;
  const unwatchPreset = watch(
    () => [options.paper, options.landscape] as const,
    ([paper, landscape], [oldPaper]) => {
      if (oldPaper === "custom") {
        customPaper = {
          widthMm: options.widthMm,
          heightMm: options.heightMm,
          marginMm: options.marginMm,
        };
      }
      if (paper === "custom") {
        // Returning to custom restores its last final dimensions; orientation is
        // deliberately not applied to custom (including while editing it).
        if (oldPaper !== "custom" && customPaper) {
          quiet = true;
          Object.assign(options, customPaper);
          quiet = false;
        }
        return;
      }
      const preset = PAPER_PRESETS[paper];
      quiet = true;
      options.widthMm = landscape ? preset.heightMm : preset.widthMm;
      options.heightMm = landscape ? preset.widthMm : preset.heightMm;
      if (paper !== oldPaper) options.marginMm = preset.marginMm;
      quiet = false;
    },
    { flush: "sync" },
  );
  const unwatchOptions = watch(
    options,
    () => {
      if (quiet || disposed) return;
      revision.value++;
      error.value = null;
      if (status.value === "loading" || !structure.value) return;
      epoch++;
      runtime.cancel();
      stopTimer();
      status.value = "idle";
      // Delay validation until the user finishes a numeric input, while stale is
      // immediate so neither SVG nor asynchronous PNG can export old options.
      timer = setTimeout(() => {
        void render();
      }, env.debounceMs ?? 180);
    },
    { deep: true, flush: "sync" },
  );
  const beginLoad = () => {
    invalidate();
    fetchAbort?.abort();
    fetchAbort = undefined;
    error.value = null;
    status.value = "loading";
    return epoch;
  };
  const acceptText = async (text: string, name: string, job: number) => {
    if (disposed || epoch !== job) return;
    const next = await runtime.parse(text, name);
    if (disposed || epoch !== job) return;
    structure.value = next;
    fileName.value = name;
    quiet = true;
    options.selectedChains = next.chains.map((chain) => chain.id);
    options.chainColors = {};
    quiet = false;
    status.value = "idle";
    await render();
  };
  const loadFile = async (file: File): Promise<void> => {
    if (disposed) return;
    const job = beginLoad();
    try {
      validateFile(file.name, file.size);
      const text = await file.text();
      await acceptText(text, file.name, job);
    } catch (cause) {
      if (!disposed && epoch === job && !isAbort(cause)) fail(cause);
    }
  };
  const loadExample = async (): Promise<void> => {
    if (disposed) return;
    const job = beginLoad();
    const controller = new AbortController();
    fetchAbort = controller;
    try {
      const response = await (env.fetch ?? fetch)(
        publicAssetUrl(env.baseURL, "examples/1crn-backbone.pdb", env.origin),
        { signal: controller.signal },
      );
      if (!response.ok)
        throw new Error(`Bundled example could not be loaded (HTTP ${response.status}).`);
      const declared = Number(response.headers.get("content-length"));
      if (declared > LIMITS.fileBytes)
        throw new Error("Bundled example exceeds the local file limit.");
      const text = await response.text();
      await acceptText(text, "1crn-backbone.pdb", job);
    } catch (cause) {
      if (!disposed && epoch === job && !isAbort(cause)) fail(cause);
    } finally {
      if (fetchAbort === controller) fetchAbort = undefined;
    }
  };
  const cancel = (): void => {
    if (disposed) return;
    invalidate();
    fetchAbort?.abort();
    fetchAbort = undefined;
    error.value = null;
    status.value = "idle";
  };
  const resetView = (): void => {
    options.rotation = [0, 0, 0];
  };
  const current = (): RenderResult => {
    if (isStale.value || !result.value)
      throw new Error("Render the current options successfully before exporting.");
    return result.value;
  };
  const downloadSvg = (): void => {
    try {
      (env.save ?? saveBlob)(svgBlob(current()), exportFileName(fileName.value, "svg"));
    } catch (cause) {
      fail(cause);
    }
  };
  const downloadPng = async (): Promise<void> => {
    const job = epoch;
    try {
      const preview = current();
      const blob = await (env.png ?? pngBlob)(preview);
      if (disposed || epoch !== job || isStale.value || result.value !== preview) return;
      (env.save ?? saveBlob)(blob, exportFileName(fileName.value, "png"));
    } catch (cause) {
      if (!disposed && epoch === job) fail(cause);
    }
  };
  const dispose = (): void => {
    if (disposed) return;
    cancel();
    disposed = true;
    unwatchOptions();
    unwatchPreset();
    runtime.dispose();
  };
  return {
    options,
    structure,
    result,
    status,
    error,
    fileName,
    isStale,
    loadFile,
    loadExample,
    render,
    cancel,
    resetView,
    downloadSvg,
    downloadPng,
    dispose,
  };
}
