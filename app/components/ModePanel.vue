<script setup lang="ts">
import { computed, inject } from "vue";
import type { RenderMode } from "~/types/render";
import type { Studio } from "../app.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";

const studio = inject<Studio>("ribbon")!;

const modeOptions: {
  value: RenderMode;
  label: string;
  icon: "ribbon" | "layers";
  title: string;
}[] = [
  {
    value: "ribbon",
    label: "Ribbon",
    icon: "ribbon",
    title: "Secondary-structure ribbons and covalent bond bands",
  },
  {
    value: "slice",
    label: "Slicer",
    icon: "layers",
    title: "Topographic molecular contour slices for pen plotting",
  },
];

const hint = computed(() =>
  studio.options.renderMode === "slice"
    ? "Topographic surface contours. Depth comes from multi-pass overdrawing and parallel lines — no line fading."
    : "Secondary-structure ribbons and covalent bond bands with hidden-line clipping.",
);
</script>

<template>
  <StudioSection title="Mode" data-test="mode-panel">
    <SegmentedControl
      v-model="studio.options.renderMode"
      legend="Rendering mode"
      hide-legend
      :options="modeOptions"
    />
    <p class="rs-hint">{{ hint }}</p>
  </StudioSection>
</template>
