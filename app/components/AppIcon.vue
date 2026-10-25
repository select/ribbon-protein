<script lang="ts">
/**
 * Small monochrome stroke icons (16px grid, currentColor). Inline so the
 * studio has no icon dependency and nothing is fetched at runtime.
 */
const ICONS = {
  upload: "M8 10.5V2.5M5 5.5l3-3 3 3M2.5 10v2a1.5 1.5 0 0 0 1.5 1.5h8a1.5 1.5 0 0 0 1.5-1.5v-2",
  file: "M9 1.75H4.25A1.5 1.5 0 0 0 2.75 3.25v9.5a1.5 1.5 0 0 0 1.5 1.5h7.5a1.5 1.5 0 0 0 1.5-1.5V5.5L9 1.75ZM9 1.75V5.5h3.75",
  flask:
    "M6 1.75h4M6.5 1.75v4L2.9 12.2a1.4 1.4 0 0 0 1.2 2.05h7.8a1.4 1.4 0 0 0 1.2-2.05L9.5 5.75v-4M4.6 9.5h6.8",
  download: "M8 2.5v8M5 7.5l3 3 3-3M2.5 10v2A1.5 1.5 0 0 0 4 13.5h8a1.5 1.5 0 0 0 1.5-1.5v-2",
  close: "M4 4l8 8M12 4l-8 8",
  reset: "M2.75 8a5.25 5.25 0 1 0 1.6-3.78M2.75 2.5v2.75H5.5",
  chevron: "M4.5 6.25 8 9.75l3.5-3.5",
  menu: "M2.5 4.5h11M2.5 8h11M2.5 11.5h11",
  alert:
    "M8 5.75v3M8 11.1v.15M6.9 2.4 1.85 11.5a1.25 1.25 0 0 0 1.1 1.85h10.1a1.25 1.25 0 0 0 1.1-1.85L9.1 2.4a1.25 1.25 0 0 0-2.2 0Z",
  info: "M8 7.25v3.75M8 5v.1M14 8A6 6 0 1 1 2 8a6 6 0 0 1 12 0Z",
  check: "M3.25 8.5 6.5 11.5l6.25-7",
  stop: "M5 5h6v6H5z",
  portrait:
    "M4.75 2h6.5c.4 0 .75.35.75.75v10.5c0 .4-.35.75-.75.75h-6.5a.75.75 0 0 1-.75-.75V2.75c0-.4.35-.75.75-.75Z",
  landscape:
    "M2.75 4h10.5c.4 0 .75.35.75.75v6.5c0 .4-.35.75-.75.75H2.75a.75.75 0 0 1-.75-.75v-6.5c0-.4.35-.75.75-.75Z",
  coaster:
    "M5 2.25h6A2.75 2.75 0 0 1 13.75 5v6A2.75 2.75 0 0 1 11 13.75H5A2.75 2.75 0 0 1 2.25 11V5A2.75 2.75 0 0 1 5 2.25Z",
  move: "M8 1.75v12.5M1.75 8h12.5M6 3.75l2-2 2 2M6 12.25l2 2 2-2M3.75 6l-2 2 2 2M12.25 6l2 2-2 2",
  image: "M2.75 3.25h10.5v9.5H2.75zM2.75 10.5l3-3 3 3 1.75-1.75 2.75 2.75M10.25 6.25v.1",
  pen: "M10.25 2.75l3 3-7.5 7.5H2.75v-3l7.5-7.5ZM8.75 4.25l3 3",
  layers: "M8 2 1.75 5.25 8 8.5l6.25-3.25L8 2ZM1.75 8 8 11.25 14.25 8M1.75 10.75 8 14l6.25-3.25",
  sliders: "M2.5 4.5h6M11.5 4.5h2M2.5 11.5h2M7.5 11.5h6M10 3v3M6 10v3",
  ribbon:
    "M1.75 10.5c1.6 0 2.4-5 4.25-5s2.4 5 4.25 5 2.4-5 4-5M1.75 13c1.6 0 2.4-5 4.25-5s2.4 5 4.25 5 2.4-5 4-5",
  lock: "M4.25 7V5.25a3.75 3.75 0 0 1 7.5 0V7M3.5 7h9v6.5h-9z",
  eye: "M1.5 8S3.9 3.5 8 3.5 14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8ZM8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z",
} as const;

export type IconName = keyof typeof ICONS;
</script>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(defineProps<{ name: IconName; size?: number; strokeWidth?: number }>(), {
  size: 16,
  strokeWidth: 1.4,
});

const path = computed(() => ICONS[props.name]);
</script>

<template>
  <svg
    class="rs-icon"
    :width="size"
    :height="size"
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    :stroke-width="strokeWidth"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <path :d="path" />
  </svg>
</template>

<style scoped>
.rs-icon {
  display: inline-block;
  flex: none;
  vertical-align: middle;
}
</style>
