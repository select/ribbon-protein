<script setup lang="ts">
import { useId } from "vue";

/** Checkbox presented as a small switch; keeps native checkbox semantics plus role="switch". */
withDefaults(defineProps<{ label: string; hint?: string; disabled?: boolean }>(), {
  hint: "",
  disabled: false,
});

const model = defineModel<boolean>({ required: true });
const id = useId();
</script>

<template>
  <div class="toggle" :class="{ 'is-disabled': disabled }">
    <label class="row" :for="id">
      <span class="rs-label">{{ label }}</span>
      <span class="switch">
        <input
          :id="id"
          v-model="model"
          class="rs-visually-hidden"
          type="checkbox"
          role="switch"
          :aria-checked="model"
          :disabled="disabled"
          :aria-describedby="hint ? `${id}-hint` : undefined"
        />
        <span class="knob" aria-hidden="true" />
      </span>
    </label>
    <p v-if="hint" :id="`${id}-hint`" class="rs-hint">{{ hint }}</p>
  </div>
</template>

<style scoped>
.toggle {
  display: grid;
  gap: 2px;
}

.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 26px;
  cursor: pointer;
}

.is-disabled .row {
  cursor: not-allowed;
}

.is-disabled .rs-label {
  color: var(--rs-faint);
}

.switch {
  position: relative;
  flex: none;
  width: 28px;
  height: 16px;
  border-radius: 999px;
  background: rgba(55, 53, 47, 0.16);
  transition: background-color 140ms var(--rs-ease);
}

.knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 2px rgba(15, 15, 15, 0.2);
  transition: transform 140ms var(--rs-ease);
}

.switch:has(input:checked) {
  background: var(--rs-ink-strong);
}

.switch:has(input:checked) .knob {
  transform: translateX(12px);
}

.switch:has(input:focus-visible) {
  outline: 2px solid var(--rs-focus);
  outline-offset: 2px;
}

.is-disabled .switch {
  opacity: 0.45;
}
</style>
