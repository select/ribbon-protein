<script setup lang="ts">
import { computed, ref, useId, watch } from "vue";

/**
 * Labelled numeric control: optional slider plus a precise number box.
 * Values are clamped and rounded to `step` before being emitted.
 */
const props = withDefaults(
  defineProps<{
    label: string;
    min: number;
    max: number;
    step?: number;
    unit?: string;
    hint?: string;
    disabled?: boolean;
    slider?: boolean;
  }>(),
  { step: 1, unit: "", hint: "", disabled: false, slider: true },
);

const model = defineModel<number>({ required: true });

const id = useId();
const hintId = `${id}-hint`;

const decimals = computed(() => {
  const text = String(props.step);
  const dot = text.indexOf(".");
  return dot < 0 ? 0 : text.length - dot - 1;
});

function format(value: number): string {
  return Number.isFinite(value) ? value.toFixed(decimals.value) : "";
}

function normalize(value: number): number {
  const clamped = Math.min(props.max, Math.max(props.min, value));
  return Number(clamped.toFixed(decimals.value));
}

const draft = ref(format(model.value));
const editing = ref(false);

watch(
  () => model.value,
  (value) => {
    if (!editing.value) draft.value = format(value);
  },
);

function onSlider(event: Event) {
  const value = Number((event.target as HTMLInputElement).value);
  if (Number.isFinite(value)) model.value = normalize(value);
}

function onNumberInput(event: Event) {
  const raw = (event.target as HTMLInputElement).value;
  draft.value = raw;
  const value = Number(raw);
  if (raw.trim() !== "" && Number.isFinite(value) && value >= props.min && value <= props.max) {
    model.value = normalize(value);
  }
}

function commit() {
  editing.value = false;
  const value = Number(draft.value);
  if (draft.value.trim() === "" || !Number.isFinite(value)) {
    draft.value = format(model.value);
    return;
  }
  const next = normalize(value);
  model.value = next;
  draft.value = format(next);
}
</script>

<template>
  <div class="range-field" :class="{ 'is-disabled': disabled, 'no-slider': !slider }">
    <div class="head">
      <label class="rs-label" :for="id">{{ label }}</label>
      <div class="number">
        <input
          :id="id"
          class="rs-input"
          type="number"
          inputmode="decimal"
          :min="min"
          :max="max"
          :step="step"
          :value="draft"
          :disabled="disabled"
          :aria-describedby="hint ? hintId : undefined"
          @focus="editing = true"
          @input="onNumberInput"
          @change="commit"
          @blur="commit"
          @keydown.enter.prevent="commit"
        />
        <span v-if="unit" class="unit" aria-hidden="true">{{ unit }}</span>
      </div>
    </div>
    <input
      v-if="slider"
      class="rs-range"
      type="range"
      :min="min"
      :max="max"
      :step="step"
      :value="model"
      :disabled="disabled"
      :aria-label="`${label} slider`"
      :aria-valuetext="`${format(model)} ${unit}`.trim()"
      @input="onSlider"
    />
    <p v-if="hint" :id="hintId" class="rs-hint">{{ hint }}</p>
  </div>
</template>

<style scoped>
.range-field {
  display: grid;
  gap: 2px;
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.is-disabled .rs-label {
  color: var(--rs-faint);
}

.number {
  position: relative;
  display: flex;
  align-items: center;
  flex: none;
  width: 92px;
}

.number .rs-input {
  min-height: 26px;
  padding-right: 30px;
}

.no-slider .number {
  width: 108px;
}

.unit {
  position: absolute;
  right: 7px;
  font-size: 11.5px;
  color: var(--rs-faint);
  pointer-events: none;
}

.rs-hint {
  margin-top: 2px;
}
</style>
