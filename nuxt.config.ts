const isGitHubPages = process.env.GITHUB_PAGES === "true";
const baseURL = process.env.NUXT_APP_BASE_URL || (isGitHubPages ? "/ribbon-protein/" : "/");
// Absolute public URLs let social crawlers resolve the card without running JS.
const siteURL = "https://select.github.io/ribbon-protein/";
const socialImage = `${siteURL}images/ribbon-studio-1ema-gfp.png`;
const description = "Local, private protein ribbon artwork for pen plotters.";
const imageAlt = "Ribbon Studio rendering green fluorescent protein (GFP, PDB 1EMA) on A4 paper.";

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
        { name: "description", content: description },
        { property: "og:type", content: "website" },
        { property: "og:site_name", content: "Ribbon Studio" },
        { property: "og:title", content: "Ribbon Studio" },
        { property: "og:description", content: description },
        { property: "og:url", content: siteURL },
        { property: "og:image", content: socialImage },
        { property: "og:image:type", content: "image/png" },
        { property: "og:image:width", content: "1440" },
        { property: "og:image:height", content: "1040" },
        { property: "og:image:alt", content: imageAlt },
        { name: "twitter:card", content: "summary_large_image" },
        { name: "twitter:title", content: "Ribbon Studio" },
        { name: "twitter:description", content: description },
        { name: "twitter:image", content: socialImage },
        { name: "twitter:image:alt", content: imageAlt },
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
        "molstar/lib/mol-math/graph",
        "molstar/lib/mol-math/linear-algebra",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/common",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/protein-info",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/trace-lookup",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/turns",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/helices",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/ladders",
        "molstar/lib/mol-model-props/computed/secondary-structure/dssp/sheets",
        "molstar/lib/mol-model/structure",
        "molstar/lib/mol-model/structure/model/types",
      ],
    },
  },
});
