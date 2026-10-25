<script setup lang="ts" generic="T extends string | number">
import { useId } from "vue";
import AppIcon, { type IconName } from "./AppIcon.vue";

/** Radio-group rendered as a compact segmented control (native arrow-key navigation). */
defineProps<{
  legend: string;
  options: { value: T; label: string; icon?: IconName; title?: string }[];
  disabled?: boolean;
  hideLegend?: boolean;
  iconOnly?: boolean;
}>();

const model = defineModel<T>({ required: true });
const name = useId();
</script>

<template>
  <fieldset class="segmented" :disabled="disabled">
    <legend :class="hideLegend ? 'rs-visually-hidden' : 'rs-label'">{{ legend }}</legend>
    <div class="track">
      <label
        v-for="option in options"
        :key="String(option.value)"
        class="segment"
        :class="{ 'is-active': model === option.value }"
        :title="option.title ?? (iconOnly ? option.label : undefined)"
      >
        <input
          class="rs-visually-hidden"
          type="radio"
          :name="name"
          :value="option.value"
          :checked="model === option.value"
          @change="model = option.value"
        />
        <AppIcon v-if="option.icon" :name="option.icon" :size="14" />
        <span :class="{ 'rs-visually-hidden': iconOnly && option.icon }">{{ option.label }}</span>
      </label>
    </div>
  </fieldset>
</template>

<style scoped>
.segmented {
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.segmented > legend.rs-label {
  margin-bottom: 6px;
  padding: 0;
}

.track {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: var(--rs-radius);
  background: rgba(55, 53, 47, 0.06);
}

.segment {
  position: relative;
  display: inline-flex;
  flex: 1 1 0;
  align-items: center;
  justify-content: center;
  gap: 5px;
  min-width: 0;
  min-height: 26px;
  padding: 0 6px;
  border-radius: var(--rs-radius-sm);
  font-size: 12.5px;
  color: var(--rs-muted);
  white-space: nowrap;
  cursor: pointer;
  user-select: none;
  transition:
    background-color 120ms var(--rs-ease),
    color 120ms var(--rs-ease);
}

.segment span {
  overflow: hidden;
  text-overflow: ellipsis;
}

.segment:hover {
  color: var(--rs-ink);
}

.segment.is-active {
  background: var(--rs-surface);
  color: var(--rs-ink-strong);
  font-weight: 500;
  box-shadow:
    0 0 0 1px var(--rs-line),
    0 1px 1px rgba(15, 15, 15, 0.04);
}

.segment:has(input:focus-visible) {
  outline: 2px solid var(--rs-focus);
  outline-offset: 1px;
}

.segmented:disabled .segment {
  color: var(--rs-faint);
  cursor: not-allowed;
}

.segmented:disabled .segment.is-active {
  background: transparent;
  box-shadow: 0 0 0 1px var(--rs-line);
}

.segmented:disabled legend {
  color: var(--rs-faint);
}
</style>
