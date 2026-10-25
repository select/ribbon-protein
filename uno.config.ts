import { defineConfig, presetIcons, presetWind4 } from "unocss";
export default defineConfig({
  presets: [
    presetWind4(),
    presetIcons({
      collections: {
        lucide: () => import("@iconify-json/lucide/icons.json").then((i) => i.default),
        mdi: () => import("@iconify-json/mdi/icons.json").then((i) => i.default),
      },
    }),
  ],
});
