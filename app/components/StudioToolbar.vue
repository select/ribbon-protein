<script setup lang="ts">
import { computed, inject, ref } from "vue";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

const exporting = ref<"svg" | "png" | null>(null);
const exportError = ref<string | null>(null);

const busy = computed(() => studio.status === "loading" || studio.status === "rendering");

const status = computed(() => {
  switch (studio.status) {
    case "loading":
      return { tone: "busy", text: "Reading structure" };
    case "rendering":
      return { tone: "busy", text: "Rendering" };
    case "error":
      return { tone: "error", text: "Error" };
    case "ready":
      return studio.isStale
        ? { tone: "pending", text: "Pending changes" }
        : { tone: "ok", text: "Up to date" };
    default:
      if (!studio.structure) return { tone: "idle", text: "No structure" };
      return { tone: "pending", text: studio.result ? "Pending changes" : "Waiting to render" };
  }
});

const exportBlocked = computed(() => {
  if (!studio.result) return "Nothing has been rendered yet";
  if (studio.isStale || busy.value) return "Waiting for the preview to match the current settings";
  if (studio.status === "error") return "Fix the error and re-render before exporting";
  return "";
});

async function download(kind: "svg" | "png") {
  if (exportBlocked.value || exporting.value) return;
  exporting.value = kind;
  exportError.value = null;
  try {
    await (kind === "svg" ? studio.downloadSvg() : studio.downloadPng());
  } catch (error) {
    exportError.value = `${kind.toUpperCase()} export failed: ${error instanceof Error ? error.message : String(error)}`;
  } finally {
    exporting.value = null;
  }
}

const title = computed(() => studio.fileName || studio.structure?.name || "Untitled");
</script>

<template>
  <header class="toolbar">
    <div class="left">
      <button
        type="button"
        class="rs-btn rs-btn--ghost rs-btn--icon menu"
        aria-controls="studio-settings"
        :aria-expanded="ui.drawerOpen"
        aria-label="Open settings"
        @click="ui.drawerOpen = true"
      >
        <AppIcon name="sliders" />
      </button>
      <h1 class="doc-title" :title="title">{{ title }}</h1>
      <span class="status" :data-tone="status.tone" role="status" aria-live="polite">
        <span v-if="status.tone === 'busy' && busy" class="rs-spinner" />
        <span v-else class="dot" aria-hidden="true" />
        {{ status.text }}
      </span>
      <button
        v-if="busy"
        type="button"
        class="rs-btn rs-btn--ghost rs-btn--sm"
        @click="studio.cancel()"
      >
        <AppIcon name="stop" :size="12" />
        Cancel
      </button>
    </div>

    <div class="right" :title="exportBlocked || undefined">
      <span v-if="exportError" class="export-error" role="alert">{{ exportError }}</span>
      <button
        type="button"
        class="rs-btn"
        :disabled="!!exportBlocked || !!exporting"
        :aria-describedby="exportBlocked ? 'export-blocked' : undefined"
        @click="download('png')"
      >
        <span v-if="exporting === 'png'" class="rs-spinner" />
        <AppIcon v-else name="image" :size="14" />
        PNG
      </button>
      <button
        type="button"
        class="rs-btn rs-btn--primary"
        :disabled="!!exportBlocked || !!exporting"
        :aria-describedby="exportBlocked ? 'export-blocked' : undefined"
        @click="download('svg')"
      >
        <span v-if="exporting === 'svg'" class="rs-spinner" />
        <AppIcon v-else name="download" :size="14" />
        SVG
      </button>
      <span id="export-blocked" class="rs-visually-hidden">{{ exportBlocked }}</span>
    </div>
  </header>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 48px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--rs-line);
  background: rgba(251, 250, 247, 0.7);
}

.left,
.right {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.right {
  flex: none;
  gap: 6px;
}

.menu {
  display: none;
}

.doc-title {
  min-width: 0;
  margin: 0;
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--rs-ink-strong);
}

.status {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: 6px;
  padding: 1px 8px;
  border-radius: 999px;
  background: rgba(55, 53, 47, 0.06);
  font-size: 12px;
  color: var(--rs-muted);
  white-space: nowrap;
}

.status .rs-spinner {
  width: 10px;
  height: 10px;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--rs-faint);
}

.status[data-tone="ok"] .dot {
  background: var(--rs-ok);
}

.status[data-tone="pending"] .dot {
  background: var(--rs-warn);
}

.status[data-tone="error"] {
  background: var(--rs-danger-bg);
  color: var(--rs-danger);
}

.status[data-tone="error"] .dot {
  background: var(--rs-danger);
}

.export-error {
  max-width: 260px;
  overflow: hidden;
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--rs-danger);
}

@media (max-width: 900px) {
  .menu {
    display: inline-flex;
  }

  .toolbar {
    padding: 8px 10px;
  }
}

@media (max-width: 520px) {
  .doc-title {
    display: none;
  }

  .export-error {
    max-width: 40vw;
  }
}
</style>
