<script setup lang="ts">
import { computed, inject } from "vue";
import { PAPER_PRESETS, type RenderOptions } from "~/types/render";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";
import ToggleSwitch from "./ToggleSwitch.vue";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

type Paper = RenderOptions["paper"];

const paperOptions: { value: Paper; label: string; title: string }[] = [
  { value: "a4", label: PAPER_PRESETS.a4.label, title: "A4 · 210 × 297 mm" },
  { value: "a3", label: PAPER_PRESETS.a3.label, title: "A3 · 297 × 420 mm" },
  { value: "bierdeckel", label: "Coaster", title: "Bierdeckel · 93.14 mm rounded square" },
  { value: "custom", label: "Custom", title: "Custom width and height" },
];

const paper = computed<Paper>({
  get: () => studio.options.paper,
  set: (value) => {
    studio.options.paper = value;
  },
});

const isCoaster = computed(() => studio.options.paper === "bierdeckel");
const isCustom = computed(() => studio.options.paper === "custom");

const orientation = computed<"portrait" | "landscape">({
  get: () => (studio.options.landscape ? "landscape" : "portrait"),
  // Only flip the flag; the runtime resolves width/height centrally.
  set: (value) => {
    studio.options.landscape = value === "landscape";
  },
});

const orientationOptions = [
  { value: "portrait" as const, label: "Portrait", icon: "portrait" as const },
  { value: "landscape" as const, label: "Landscape", icon: "landscape" as const },
];

const width = computed({
  get: () => studio.options.widthMm,
  set: (value: number) => {
    studio.options.widthMm = value;
  },
});

const height = computed({
  get: () => studio.options.heightMm,
  set: (value: number) => {
    studio.options.heightMm = value;
  },
});

const margin = computed({
  get: () => studio.options.marginMm,
  set: (value: number) => {
    studio.options.marginMm = value;
  },
});

/** Custom sizes are not re-oriented centrally, so swapping is explicit here. */
function swapCustom() {
  const { widthMm, heightMm } = studio.options;
  studio.options.widthMm = heightMm;
  studio.options.heightMm = widthMm;
}

const marginMax = computed(() => {
  const shortest = Math.min(studio.options.widthMm, studio.options.heightMm);
  return Math.max(0, Math.floor(shortest / 2 - 1));
});

const showGuides = computed({
  get: () => ui.showGuides,
  set: (value: boolean) => {
    ui.showGuides = value;
  },
});

function mm(value: number): string {
  return String(Number(value.toFixed(2)));
}

const summary = computed(() => {
  const { widthMm, heightMm, marginMm } = studio.options;
  const drawW = Math.max(0, widthMm - 2 * marginMm);
  const drawH = Math.max(0, heightMm - 2 * marginMm);
  return `${mm(widthMm)} × ${mm(heightMm)} mm sheet · ${mm(drawW)} × ${mm(drawH)} mm drawing area`;
});
</script>

<template>
  <StudioSection title="Paper">
    <SegmentedControl v-model="paper" legend="Format" :options="paperOptions" />

    <SegmentedControl
      v-if="!isCoaster && !isCustom"
      v-model="orientation"
      legend="Orientation"
      :options="orientationOptions"
    />

    <div v-if="isCustom" class="dims">
      <RangeField
        v-model="width"
        label="Width"
        :min="10"
        :max="2000"
        :step="1"
        unit="mm"
        :slider="false"
      />
      <RangeField
        v-model="height"
        label="Height"
        :min="10"
        :max="2000"
        :step="1"
        unit="mm"
        :slider="false"
      />
      <button type="button" class="rs-btn rs-btn--sm swap" @click="swapCustom">
        <AppIcon :name="width > height ? 'portrait' : 'landscape'" :size="12" />
        Swap width and height
      </button>
    </div>

    <RangeField v-model="margin" label="Margin" :min="0" :max="marginMax" :step="0.5" unit="mm" />

    <p class="rs-hint rs-num summary">{{ summary }}</p>

    <p v-if="isCoaster" class="rs-hint">
      Single rounded-square Bierdeckel guide, 92.74 mm with 10 mm corners. The outline is a preview
      reference only and is never included in exports.
      <span v-if="margin < 3.14" role="status">
        Below 3.14 mm margin, artwork can extend beyond the rounded corners of the physical coaster.
        The export is not clipped to the guide.
      </span>
    </p>

    <ToggleSwitch
      v-model="showGuides"
      label="Show guides"
      hint="Margin and coaster outlines in the preview only."
    />
  </StudioSection>
</template>

<style scoped>
.dims {
  display: grid;
  gap: 8px;
}

.swap {
  justify-self: start;
}

.summary {
  margin-top: -6px;
  font-family: var(--rs-font);
  font-size: 12px;
}
</style>
