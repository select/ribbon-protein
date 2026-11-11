<script setup lang="ts">
import { computed, inject } from "vue";
import type { SliceAxis, Vec3 } from "~/types/render";
import type { Studio } from "../app.vue";
import { hasSelectedMode } from "../lib/molecule-groups";
import AppIcon from "./AppIcon.vue";
import DrawingGroup from "./DrawingGroup.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";

const studio = inject<Studio>("ribbon")!;

function axis(index: 0 | 1 | 2) {
  return computed({
    get: () => studio.options.rotation[index],
    set: (value: number) => {
      const next = [...studio.options.rotation] as Vec3;
      next[index] = value;
      studio.options.rotation = next;
    },
  });
}

function sliceTilt(index: 0 | 1 | 2) {
  return computed({
    get: () => studio.options.sliceRotation[index],
    set: (value: number) => {
      const next = [...studio.options.sliceRotation] as Vec3;
      next[index] = value;
      studio.options.sliceRotation = next;
    },
  });
}

const axes = [
  { name: "X", model: axis(0) },
  { name: "Y", model: axis(1) },
  { name: "Z", model: axis(2) },
];

const sliceAxesTilt = [
  { name: "X", model: sliceTilt(0) },
  { name: "Y", model: sliceTilt(1) },
  { name: "Z", model: sliceTilt(2) },
];

const isDefault = computed(() => studio.options.rotation.every((value) => value === 0));
const isSliceDefault = computed(() => studio.options.sliceRotation.every((value) => value === 0));
const isSlice = computed(() => hasSelectedMode(studio.options, "slice"));
const sliceAxes: { value: SliceAxis; label: string; title: string }[] = [
  { value: "x", label: "X", title: "Slice perpendicular to structure X axis" },
  { value: "y", label: "Y", title: "Slice perpendicular to structure Y axis" },
  { value: "z", label: "Z", title: "Slice perpendicular to structure Z axis" },
  { value: "view", label: "View", title: "Slice front-to-back along the camera direction" },
];

function resetSliceDirection(): void {
  studio.options.sliceRotation = [0, 0, 0];
}
</script>

<template>
  <StudioSection title="View">
    <template #actions>
      <button
        type="button"
        class="rs-btn rs-btn--ghost rs-btn--sm"
        :disabled="isDefault"
        title="Reset to the principal-axis view"
        @click="studio.resetView()"
      >
        <AppIcon name="reset" :size="12" />
        Reset
      </button>
    </template>

    <RangeField
      v-for="item in axes"
      :key="item.name"
      v-model="item.model.value"
      :label="`Rotate ${item.name}`"
      :min="-180"
      :max="180"
      :step="1"
      unit="°"
    />

    <DrawingGroup
      v-if="isSlice"
      title="Slice direction"
      default-open
      data-category="slice-direction"
    >
      <SegmentedControl
        v-model="studio.options.sliceAxis"
        legend="Section axis"
        :options="sliceAxes"
      />
      <RangeField
        v-for="item in sliceAxesTilt"
        :key="`slice-${item.name}`"
        v-model="item.model.value"
        :label="`Slice ${item.name}`"
        :min="-180"
        :max="180"
        :step="1"
        unit="°"
      />
      <button
        v-if="!isSliceDefault"
        type="button"
        class="rs-btn rs-btn--ghost rs-btn--sm slice-reset"
        @click="resetSliceDirection"
      >
        <AppIcon name="reset" :size="12" />
        Reset slice direction
      </button>
    </DrawingGroup>

    <p class="rs-hint">
      <AppIcon name="move" :size="12" class="inline-icon" />
      Drag the preview to rotate, or focus it and use the arrow keys (Shift for 15°, [ and ] for Z).
    </p>
  </StudioSection>
</template>

<style scoped>
.inline-icon {
  margin: -2px 3px 0 0;
}

.slice-reset {
  justify-self: start;
}
</style>
