<script setup lang="ts">
import { computed, inject } from "vue";
import type { RenderOptions } from "~/types/render";
import type { Studio } from "../app.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";
import ToggleSwitch from "./ToggleSwitch.vue";

const studio = inject<Studio>("ribbon")!;
const isSlice = computed(() => studio.options.renderMode === "slice");
const directionOptions = computed(
  (): { value: RenderOptions["strokeDirection"]; label: string; title: string }[] => [
    {
      value: "backbone",
      label: isSlice.value ? "Contour" : "Backbone",
      title: isSlice.value
        ? "Preserve slice and contour winding order"
        : "Smooth forward joins, paths never reversed",
    },
    { value: "travel", label: "Travel", title: "Minimise pen-up travel, paths may be reversed" },
  ],
);
const directionHint = computed(() => {
  if (studio.options.strokeDirection === "travel") {
    return "Paths may be reversed and reordered to minimise pen-up travel.";
  }
  return isSlice.value
    ? "Slices plot from back to front, keeping multi-pass overdraws grouped per track."
    : "Paths follow the chain direction with smooth forward joins.";
});
</script>

<template>
  <StudioSection title="Advanced" collapsible :default-open="false">
    <template v-if="isSlice">
      <RangeField
        v-model="studio.options.sliceInkOpacity"
        label="Preview ink opacity"
        :min="0.1"
        :max="1"
        :step="0.05"
        hint="Simulates physical ink buildup in the SVG preview where strokes overlap or retrace."
      />
      <ToggleSwitch
        v-model="studio.options.slicePerspective"
        label="Perspective"
        hint="Foreshorten deeper contour slices from the camera."
      />
    </template>
    <RangeField
      v-else
      v-model="studio.options.samples"
      label="Samples per residue"
      :min="2"
      :max="64"
      :step="1"
      hint="Higher is smoother and slower."
    />
    <div class="direction">
      <SegmentedControl
        v-model="studio.options.strokeDirection"
        legend="Stroke order"
        :options="directionOptions"
      />
      <p class="rs-hint">{{ directionHint }}</p>
    </div>
  </StudioSection>
</template>

<style scoped>
.direction {
  display: grid;
  gap: 4px;
}
</style>
