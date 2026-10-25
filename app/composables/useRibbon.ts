import { onScopeDispose } from "vue";
import { createRibbonStudio } from "../lib/ribbon-studio";
import { RibbonWorkerClient, publicAssetUrl } from "../lib/worker-client";

export function useRibbon() {
  const config = useRuntimeConfig();
  const baseURL = config.app.baseURL;
  const origin = window.location.origin;
  const runtime = new RibbonWorkerClient(
    () => new Worker(new URL("../workers/ribbon.worker.ts", import.meta.url), { type: "module" }),
    publicAssetUrl(baseURL, "wasm/ribbon_core.js", origin),
  );
  const { dispose, ...studio } = createRibbonStudio(runtime, { baseURL, origin });
  onScopeDispose(dispose);
  return studio;
}
