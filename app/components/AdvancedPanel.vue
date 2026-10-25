<script setup lang="ts">
import { computed, inject } from "vue";
import type { RenderOptions } from "~/types/render";
import type { Studio } from "../app.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";
import ToggleSwitch from "./ToggleSwitch.vue";

const studio = inject<Studio>("ribbon")!;

type NumericKey = "loopSpacingMm" | "borderSpacingMm" | "samples" | "borderPasses";

function numeric(key: NumericKey) {
  return computed({
    get: () => studio.options[key],
    set: (value: number) => {
      studio.options[key] = value;
    },
  });
}

const loopSpacing = numeric("loopSpacingMm");
const borderSpacing = numeric("borderSpacingMm");
const samples = numeric("samples");
const borderPasses = numeric("borderPasses");

const fillLoops = computed({
  get: () => studio.options.fillLoops,
  set: (value: boolean) => {
    studio.options.fillLoops = value;
  },
});

const direction = computed<RenderOptions["strokeDirection"]>({
  get: () => studio.options.strokeDirection,
  set: (value) => {
    studio.options.strokeDirection = value;
  },
});

const passOptions = [
  { value: 1, label: "1" },
  { value: 2, label: "2" },
  { value: 3, label: "3" },
];

const directionOptions: {
  value: RenderOptions["strokeDirection"];
  label: string;
  title: string;
}[] = [
  { value: "backbone", label: "Backbone", title: "Smooth forward joins, paths never reversed" },
  { value: "travel", label: "Travel", title: "Minimise pen-up travel, paths may be reversed" },
];

const directionHint = computed(() =>
  studio.options.strokeDirection === "backbone"
    ? "Paths follow the chain direction with smooth forward joins."
    : "Paths may be reversed to minimise pen-up travel.",
);
</script>

<template>
  <StudioSection title="Advanced" collapsible :default-open="false">
    <ToggleSwitch
      v-model="fillLoops"
      label="Fill loops"
      hint="Draw unstructured loop tubes with close parallel paths."
    />
    <RangeField
      v-model="loopSpacing"
      label="Loop spacing"
      :min="0.02"
      :max="1"
      :step="0.01"
      unit="mm"
      :disabled="!fillLoops"
    />

    <SegmentedControl v-model="borderPasses" legend="Border passes" :options="passOptions" />
    <RangeField
      v-model="borderSpacing"
      label="Border spacing"
      :min="0.01"
      :max="0.5"
      :step="0.01"
      unit="mm"
      :disabled="borderPasses < 2"
      hint="Inset between parallel helix and sheet outlines."
    />

    <RangeField
      v-model="samples"
      label="Samples per residue"
      :min="2"
      :max="64"
      :step="1"
      hint="Higher is smoother and slower."
    />

    <div class="direction">
      <SegmentedControl v-model="direction" legend="Stroke order" :options="directionOptions" />
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
