<script setup lang="ts">
withDefaults(defineProps<{ title: string; defaultOpen?: boolean }>(), { defaultOpen: false });
</script>

<template>
  <details class="drawing-group" :open="defaultOpen">
    <summary tabindex="0">{{ title }}</summary>
    <div class="drawing-controls"><slot /></div>
  </details>
</template>

<style scoped>
.drawing-group {
  min-width: 0;
  border: 1px solid var(--rs-line);
  border-radius: var(--rs-radius);
}
summary {
  padding: 10px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  color: var(--rs-ink-strong);
}
summary:hover {
  background: var(--rs-hover);
}
summary:focus-visible {
  outline: 2px solid var(--rs-ink);
  outline-offset: 2px;
}
.drawing-controls {
  display: grid;
  gap: 12px;
  padding: 2px 10px 12px;
}
</style>
