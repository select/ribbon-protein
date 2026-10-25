import { reactive, ref, shallowRef, computed, watch, triggerRef } from "vue";
import { defaultRenderOptions, PAPER_PRESETS } from "../types/render";
import type {
  PreviewScene,
  RenderOptions,
  StructureData,
  RenderResult,
  RenderStatus,
} from "../types/render";
import { previewGeometryKey, renderInteractionPreview } from "./interaction-preview";
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
  const previewResult = shallowRef<RenderResult | null>(null);
  const interacting = ref(false);
  const displayResult = computed(() => previewResult.value ?? result.value);
  let requestedAt = performance.now();
  let scene: { source: StructureData; key: string; data: PreviewScene } | undefined;
  const received = new WeakMap<RenderResult, { requested: number; received: number }>();
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
    () =>
      interacting.value ||
      !result.value ||
      revision.value !== renderedRevision.value ||
      status.value !== "ready",
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
    runtime.invalidate();
    revision.value++;
  };
  const render = async (): Promise<void> => {
    if (disposed || status.value === "loading") return;
    stopTimer();
    const source = structure.value;
    if (!source) return;
    invalidate();
    const job = epoch,
      requestedRevision = revision.value,
      quality = interacting.value ? "preview" : "final",
      started = requestedAt;
    try {
      const snapshot = JSON.parse(JSON.stringify(options)) as RenderOptions;
      validateOptions(snapshot, source);
      error.value = null;
      status.value = "rendering";
      const key = previewGeometryKey(snapshot);
      const next =
        quality === "preview" && scene?.source === source && scene.key === key
          ? renderInteractionPreview(scene.data, snapshot)
          : await runtime.render({ structure: source, options: snapshot }, quality);
      if (disposed || epoch !== job) return;
      if (next.previewScene) scene = { source, key, data: next.previewScene };
      next.quality = quality;
      next.timings = { ...next.timings, responseMs: performance.now() - started };
      received.set(next, { requested: started, received: performance.now() });
      if (quality === "preview") {
        previewResult.value = next;
        // A preview is display-only. It never updates renderedRevision/result.
        status.value = "idle";
      } else {
        previewResult.value = null;
        result.value = next;
        renderedRevision.value = requestedRevision;
        status.value = "ready";
      }
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
      requestedAt = performance.now();
      error.value = null;
      if (status.value === "loading" || !structure.value) return;
      epoch++;
      runtime.invalidate();
      status.value = "idle";
      // Interaction is frame-coalesced, NOT trailing-debounced: a continuous
      // pointer stream must not postpone every preview until the pointer stops.
      // Discrete/numeric edits still debounce; export staleness is immediate.
      if (interacting.value) {
        if (!timer)
          timer = setTimeout(() => {
            void render();
          }, 16);
      } else {
        stopTimer();
        timer = setTimeout(() => {
          void render();
        }, env.debounceMs ?? 180);
      }
    },
    { deep: true, flush: "sync" },
  );
  const beginLoad = () => {
    if (status.value === "loading") runtime.cancel();
    interacting.value = false;
    previewResult.value = null;
    scene = undefined;
    requestedAt = performance.now();
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
    const water = new Set(
      next.molecules?.filter((m) => m.category === "water").map((m) => m.chainId),
    );
    options.selectedChains = next.chains
      .filter((chain) => !water.has(chain.id))
      .map((chain) => chain.id);
    options.chainColors = Object.fromEntries(
      (next.molecules ?? [])
        .filter((m) => m.category === "lipid")
        .map((m) => [m.chainId, "#c4302b"]),
    );
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
    interacting.value = false;
    previewResult.value = null;
    invalidate();
    runtime.cancel();
    fetchAbort?.abort();
    fetchAbort = undefined;
    error.value = null;
    status.value = "idle";
  };
  const beginInteraction = (): void => {
    if (disposed || !structure.value || status.value === "loading") return;
    interacting.value = true;
    requestedAt = performance.now();
    invalidate();
  };
  const endInteraction = (): void => {
    if (disposed || !interacting.value) return;
    interacting.value = false;
    requestedAt = performance.now();
    invalidate();
    void render();
  };
  const markDisplayed = (next: RenderResult): void => {
    const timing = received.get(next);
    if (disposed || displayResult.value !== next || !timing) return;
    next.timings = {
      ...next.timings,
      imageDecodeMs: performance.now() - timing.received,
      visibleMs: performance.now() - timing.requested,
    };
    triggerRef(result);
    triggerRef(previewResult);
  };
  const resetView = (): void => {
    interacting.value = false;
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
    scene = undefined;
  };
  return {
    options,
    structure,
    result,
    previewResult,
    displayResult,
    interacting,
    beginInteraction,
    endInteraction,
    markDisplayed,
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
