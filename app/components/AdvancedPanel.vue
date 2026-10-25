<script setup lang="ts">
import { computed, inject } from "vue";
import type { RenderOptions } from "~/types/render";
import type { Studio } from "../app.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";

const studio = inject<Studio>("ribbon")!;
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
    <RangeField
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
