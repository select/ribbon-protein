const isGitHubPages = process.env.GITHUB_PAGES === "true";
const baseURL = process.env.NUXT_APP_BASE_URL || (isGitHubPages ? "/ribbon-protein/" : "/");

export default defineNuxtConfig({
  compatibilityDate: "2026-04-01",
  ssr: false,
  nitro: { preset: isGitHubPages ? "github-pages" : undefined },
  // Nuxt 4.4's serial Vite path looks for a nonexistent SSR entry with ssr:false.
  // Environment API keeps the SPA dev runner and client entry distinct.
  experimental: { viteEnvironmentApi: true },
  devtools: { enabled: false },
  modules: ["@unocss/nuxt"],
  css: ["@unocss/reset/tailwind.css", "~/assets/css/main.css"],
  app: {
    baseURL,
    head: {
      title: "Ribbon Studio",
      htmlAttrs: { lang: "en" },
      link: [{ rel: "icon", type: "image/svg+xml", href: `${baseURL}favicon.svg` }],
      meta: [
        { name: "description", content: "Local, private protein ribbon artwork for pen plotters." },
      ],
    },
  },
  vite: {
    worker: { format: "es" },
    // Worker imports are discovered only after the first local file is loaded.
    // Prebundle them at startup so Vite does not reload the SPA mid-calculation.
    optimizeDeps: {
      include: [
        "molstar/lib/mol-data/int",
        "molstar/lib/mol-io/reader/cif",
        "molstar/lib/mol-io/reader/pdb/parser",
        "molstar/lib/mol-model-formats/structure/mmcif",
        "molstar/lib/mol-model-formats/structure/pdb",
        "molstar/lib/mol-model-props/computed/secondary-structure/zhang-skolnik",
        "molstar/lib/mol-model/structure",
        "molstar/lib/mol-model/structure/model/types",
      ],
    },
  },
});
