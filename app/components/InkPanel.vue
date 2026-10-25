<script setup lang="ts">
import { computed, inject } from "vue";
import type { Studio } from "../app.vue";
import RangeField from "./RangeField.vue";
import StudioSection from "./StudioSection.vue";

const studio = inject<Studio>("ribbon")!;

const lines = computed({
  get: () => studio.options.lines,
  set: (value: number) => {
    studio.options.lines = Math.round(value);
  },
});

const pen = computed({
  get: () => studio.options.penWidthMm,
  set: (value: number) => {
    studio.options.penWidthMm = value;
  },
});
</script>

<template>
  <StudioSection title="Ink">
    <RangeField
      v-model="lines"
      label="Interior lines"
      :min="0"
      :max="20"
      :step="1"
      hint="Contour lines inside each helix and sheet ribbon."
    />
    <RangeField
      v-model="pen"
      label="Pen width"
      :min="0.1"
      :max="2"
      :step="0.05"
      unit="mm"
      hint="Preview stroke width. Paths stay centerlines; the physical pen decides the real ink width."
    />
  </StudioSection>
</template>
