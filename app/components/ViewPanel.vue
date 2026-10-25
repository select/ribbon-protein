<script setup lang="ts">
import { computed, inject } from "vue";
import type { Vec3 } from "~/types/render";
import type { Studio } from "../app.vue";
import AppIcon from "./AppIcon.vue";
import RangeField from "./RangeField.vue";
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

const axes = [
  { name: "X", model: axis(0) },
  { name: "Y", model: axis(1) },
  { name: "Z", model: axis(2) },
];

const isDefault = computed(() => studio.options.rotation.every((value) => value === 0));
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
</style>
