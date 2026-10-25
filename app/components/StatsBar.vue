<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from "vue";
import type { Studio } from "../app.vue";
import AppIcon from "./AppIcon.vue";

const studio = inject<Studio>("ribbon")!;

const showWarnings = ref(false);
const warningsEl = ref<HTMLElement | null>(null);

function onOutside(event: PointerEvent) {
  if (warningsEl.value && !warningsEl.value.contains(event.target as Node))
    showWarnings.value = false;
}

function onEscape(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  showWarnings.value = false;
  warningsEl.value?.querySelector<HTMLElement>("button")?.focus();
}

function unlisten() {
  document.removeEventListener("pointerdown", onOutside);
  document.removeEventListener("keydown", onEscape);
}

watch(showWarnings, (open) => {
  unlisten();
  if (open) {
    document.addEventListener("pointerdown", onOutside);
    document.addEventListener("keydown", onEscape);
  }
});

onBeforeUnmount(unlisten);

const warnings = computed(() => {
  const all = [...(studio.structure?.warnings ?? []), ...(studio.result?.warnings ?? [])];
  return [...new Set(all.map((warning) => warning.trim()).filter(Boolean))];
});

function trimNumber(value: number, digits: number): string {
  return String(Number(value.toFixed(digits)));
}

function formatLength(mm: number): string {
  if (mm >= 1000) return `${trimNumber(mm / 1000, 2)} m`;
  if (mm >= 10) return `${trimNumber(mm / 10, 1)} cm`;
  return `${trimNumber(mm, 1)} mm`;
}

function formatTime(ms: number): string {
  return ms >= 1000 ? `${trimNumber(ms / 1000, 2)} s` : `${Math.round(ms)} ms`;
}

const stats = computed(() => {
  const result = studio.result;
  if (!result) return [];
  return [
    {
      label: "Size",
      value: `${trimNumber(result.widthMm, 2)} × ${trimNumber(result.heightMm, 2)} mm`,
    },
    { label: "Strokes", value: result.strokeCount.toLocaleString() },
    { label: "Pen distance", value: formatLength(result.lengthMm) },
    { label: "Residues", value: result.residueCount.toLocaleString() },
    { label: "Render", value: formatTime(result.elapsedMs) },
  ];
});
</script>

<template>
  <footer v-if="studio.structure" class="stats" aria-label="Render statistics">
    <dl class="list" :class="{ 'is-stale': studio.isStale }">
      <div v-for="item in stats" :key="item.label" class="item">
        <dt>{{ item.label }}</dt>
        <dd class="rs-num">{{ item.value }}</dd>
      </div>
      <div v-if="!stats.length" class="item">
        <dt>Not rendered yet</dt>
      </div>
    </dl>

    <div v-if="warnings.length" ref="warningsEl" class="warnings">
      <button
        type="button"
        class="rs-btn rs-btn--ghost rs-btn--sm warn-toggle"
        :aria-expanded="showWarnings"
        aria-controls="render-warnings"
        @click="showWarnings = !showWarnings"
      >
        <AppIcon name="alert" :size="12" />
        {{ warnings.length }} {{ warnings.length === 1 ? "note" : "notes" }}
      </button>
      <ul v-show="showWarnings" id="render-warnings" class="warn-list">
        <li v-for="warning in warnings" :key="warning">{{ warning }}</li>
      </ul>
    </div>
  </footer>
</template>

<style scoped>
.stats {
  position: relative;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 4px 16px;
  min-height: 36px;
  padding: 6px 16px;
  border-top: 1px solid var(--rs-line);
  background: rgba(251, 250, 247, 0.7);
  font-size: 12px;
}

.list {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 18px;
  min-width: 0;
  margin: 0;
  transition: opacity 160ms var(--rs-ease);
}

.list.is-stale {
  opacity: 0.55;
}

.item {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

dt {
  color: var(--rs-muted);
}

dd {
  margin: 0;
  color: var(--rs-ink-strong);
}

.warnings {
  position: relative;
}

.warn-toggle {
  color: var(--rs-warn);
}

.warn-list {
  position: absolute;
  right: 0;
  bottom: calc(100% + 8px);
  z-index: 5;
  width: min(380px, calc(100vw - 24px));
  max-height: 40vh;
  margin: 0;
  padding: 8px 12px 8px 26px;
  overflow-y: auto;
  border: 1px solid var(--rs-line);
  border-radius: var(--rs-radius);
  background: var(--rs-surface);
  box-shadow: 0 8px 24px -10px rgba(55, 53, 47, 0.25);
  font-size: 12.5px;
  line-height: 1.45;
  color: var(--rs-ink);
}

.warn-list li + li {
  margin-top: 4px;
}

@media (max-width: 900px) {
  .stats {
    padding: 6px 10px;
  }
}
</style>
