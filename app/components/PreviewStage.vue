<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import type { Vec3 } from "~/types/render";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

/* Bierdeckel guide geometry from public/templates/bierdeckel.svg (rect6). */
const COASTER = { canvas: 93.14, inset: 0.2, size: 92.74, radius: 10, stroke: 0.4 } as const;

/* ------------------------------------------------------------- fit to stage */

const stage = ref<HTMLElement | null>(null);
const box = reactive({ width: 0, height: 0 });
let observer: ResizeObserver | null = null;

onMounted(() => {
  if (!stage.value) return;
  observer = new ResizeObserver(([entry]) => {
    if (!entry) return;
    box.width = entry.contentRect.width;
    box.height = entry.contentRect.height;
  });
  observer.observe(stage.value);
});

const sheet = computed(() => {
  const widthMm =
    Number.isFinite(studio.options.widthMm) && studio.options.widthMm > 0
      ? studio.options.widthMm
      : 210;
  const heightMm =
    Number.isFinite(studio.options.heightMm) && studio.options.heightMm > 0
      ? studio.options.heightMm
      : 297;
  const pad = box.width < 640 ? 20 : 48;
  const availableW = Math.max(box.width - pad * 2, 60);
  const availableH = Math.max(box.height - pad * 2, 60);
  const scale = Math.min(availableW / widthMm, availableH / heightMm);
  return {
    widthMm,
    heightMm,
    scale,
    width: Math.floor(widthMm * scale),
    height: Math.floor(heightMm * scale),
  };
});

const isCoaster = computed(() => studio.options.paper === "bierdeckel");

const marginStyle = computed(() => {
  const { widthMm, heightMm } = sheet.value;
  const margin = Math.max(0, studio.options.marginMm);
  return {
    left: `${(margin / widthMm) * 100}%`,
    right: `${(margin / widthMm) * 100}%`,
    top: `${(margin / heightMm) * 100}%`,
    bottom: `${(margin / heightMm) * 100}%`,
  };
});

/* --------------------------------------------------- safe SVG via blob URL */

const svgUrl = ref<string | null>(null);

watch(
  () => studio.result?.svg ?? null,
  (svg) => {
    if (svgUrl.value) URL.revokeObjectURL(svgUrl.value);
    svgUrl.value = svg ? URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" })) : null;
  },
  { immediate: true },
);

/* ---------------------------------------------------------- drag to rotate */

const DEG_PER_PX = 0.5;
let drag: { pointerId: number; x: number; y: number; start: Vec3 } | null = null;
const dragging = ref(false);

function wrap(angle: number): number {
  return Math.round(((((angle + 180) % 360) + 360) % 360) - 180);
}

function onPointerDown(event: PointerEvent) {
  if (!studio.structure || event.button !== 0) return;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  drag = {
    pointerId: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    start: [...studio.options.rotation] as Vec3,
  };
  dragging.value = true;
}

function onPointerMove(event: PointerEvent) {
  if (!drag || event.pointerId !== drag.pointerId) return;
  const dx = event.clientX - drag.x;
  const dy = event.clientY - drag.y;
  const next: Vec3 = [
    wrap(drag.start[0] + dy * DEG_PER_PX),
    wrap(drag.start[1] + dx * DEG_PER_PX),
    drag.start[2],
  ];
  const current = studio.options.rotation;
  if (next[0] !== current[0] || next[1] !== current[1]) studio.options.rotation = next;
}

function onPointerUp(event: PointerEvent) {
  if (!drag || event.pointerId !== drag.pointerId) return;
  drag = null;
  dragging.value = false;
}

function onKeydown(event: KeyboardEvent) {
  if (!studio.structure) return;
  const step = event.shiftKey ? 15 : 5;
  const [x, y, z] = studio.options.rotation;
  let next: Vec3 | null = null;
  switch (event.key) {
    case "ArrowLeft":
      next = [x, wrap(y - step), z];
      break;
    case "ArrowRight":
      next = [x, wrap(y + step), z];
      break;
    case "ArrowUp":
      next = [wrap(x - step), y, z];
      break;
    case "ArrowDown":
      next = [wrap(x + step), y, z];
      break;
    case "[":
    case "{":
      next = [x, y, wrap(z - step)];
      break;
    case "]":
    case "}":
      next = [x, y, wrap(z + step)];
      break;
    case "Home":
    case "0":
      event.preventDefault();
      studio.resetView();
      return;
    default:
      return;
  }
  event.preventDefault();
  studio.options.rotation = next;
}

const rotationText = computed(() =>
  studio.options.rotation.map((value) => `${Math.round(value)}°`).join(" · "),
);

/* -------------------------------------------------------------- state text */

const isLoading = computed(() => studio.status === "loading");
const isRendering = computed(() => studio.status === "rendering");
const hasStructure = computed(() => studio.structure !== null);
const noChains = computed(() => hasStructure.value && studio.options.selectedChains.length === 0);

const previewLabel = computed(() => {
  const name = studio.fileName || studio.structure?.name || "structure";
  return `Ribbon preview of ${name}, rotation ${rotationText.value}. Drag or use arrow keys to rotate.`;
});

onBeforeUnmount(() => {
  observer?.disconnect();
  if (svgUrl.value) URL.revokeObjectURL(svgUrl.value);
});
</script>

<template>
  <div ref="stage" class="stage" :class="{ 'has-structure': hasStructure }">
    <!-- Empty: drop zone -->
    <div v-if="!hasStructure && !isLoading" class="empty">
      <div class="drop" :class="{ 'is-over': ui.isDragging }">
        <AppIcon name="ribbon" :size="28" :stroke-width="1.1" class="drop-mark" />
        <h2 class="drop-title">Drop a protein structure</h2>
        <p class="drop-text">
          PDB, ENT, CIF or mmCIF. Everything is parsed and drawn locally in your browser — nothing
          is uploaded.
        </p>
        <div class="drop-actions">
          <button type="button" class="rs-btn rs-btn--primary" @click="ui.pickFile()">
            <AppIcon name="upload" :size="14" />
            Choose file…
          </button>
          <button type="button" class="rs-btn" @click="studio.loadExample()">
            <AppIcon name="flask" :size="14" />
            Load example
          </button>
        </div>
      </div>
    </div>

    <!-- Parsing -->
    <div v-else-if="!hasStructure && isLoading" class="empty">
      <div class="loading" role="status">
        <span class="rs-spinner" />
        <span>Reading {{ studio.fileName || "structure" }} locally…</span>
        <button type="button" class="rs-btn rs-btn--sm" @click="studio.cancel()">Cancel</button>
      </div>
    </div>

    <!-- Paper -->
    <div
      v-else
      class="sheet"
      :class="{
        'is-coaster': isCoaster,
        'is-dragging': dragging,
        'is-stale': studio.isStale && !!svgUrl,
      }"
      :style="{ width: `${sheet.width}px`, height: `${sheet.height}px` }"
      tabindex="0"
      role="application"
      aria-roledescription="rotatable preview"
      :aria-label="previewLabel"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @lostpointercapture="onPointerUp"
      @keydown="onKeydown"
    >
      <img v-if="svgUrl" class="art" :src="svgUrl" alt="" draggable="false" />

      <div v-if="ui.showGuides" class="guide-margin" :style="marginStyle" aria-hidden="true" />

      <svg
        v-if="isCoaster && ui.showGuides"
        class="guide-coaster"
        :viewBox="`0 0 ${COASTER.canvas} ${COASTER.canvas}`"
        aria-hidden="true"
      >
        <rect
          :x="COASTER.inset"
          :y="COASTER.inset"
          :width="COASTER.size"
          :height="COASTER.size"
          :rx="COASTER.radius"
          :ry="COASTER.radius"
          fill="none"
          vector-effect="non-scaling-stroke"
        />
      </svg>

      <div v-if="!svgUrl" class="sheet-note">
        <template v-if="noChains">No chains selected</template>
        <template v-else-if="isRendering || isLoading">
          <span class="rs-spinner" />
          Drawing…
        </template>
        <template v-else-if="studio.status === 'error'">Nothing rendered</template>
        <template v-else>Waiting to render</template>
      </div>

      <div v-if="dragging" class="readout rs-num" aria-hidden="true">{{ rotationText }}</div>
    </div>

    <div v-if="hasStructure && (isRendering || isLoading) && svgUrl" class="busy" role="status">
      <span class="rs-spinner" />
      {{ isLoading ? "Reading…" : "Rendering…" }}
    </div>
  </div>
</template>

<style scoped>
.stage {
  position: relative;
  display: grid;
  flex: 1 1 auto;
  place-items: center;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

/* ------------------------------------------------------------- empty state */

.empty {
  display: grid;
  place-items: center;
  width: 100%;
  height: 100%;
  padding: 24px;
}

.drop {
  display: grid;
  justify-items: center;
  gap: 8px;
  width: min(440px, 100%);
  padding: 40px 28px 32px;
  border: 1px dashed var(--rs-line-strong);
  border-radius: 10px;
  text-align: center;
  background: rgba(255, 254, 251, 0.6);
  transition:
    border-color 140ms var(--rs-ease),
    background-color 140ms var(--rs-ease);
}

.drop.is-over {
  border-color: var(--rs-ink);
  background: var(--rs-paper);
}

.drop-mark {
  margin-bottom: 4px;
  color: var(--rs-ink);
}

.drop-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--rs-ink-strong);
}

.drop-text {
  max-width: 320px;
  margin: 0 0 10px;
  font-size: 13px;
  color: var(--rs-muted);
}

.drop-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 6px;
}

.loading {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
  color: var(--rs-muted);
}

/* ------------------------------------------------------------------- sheet */

.sheet {
  position: relative;
  overflow: hidden;
  background: var(--rs-paper);
  box-shadow:
    0 0 0 1px rgba(55, 53, 47, 0.1),
    0 1px 2px rgba(55, 53, 47, 0.06),
    0 8px 24px -12px rgba(55, 53, 47, 0.18);
  cursor: grab;
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  transition:
    width 180ms var(--rs-ease),
    height 180ms var(--rs-ease),
    border-radius 180ms var(--rs-ease);
}

.sheet.is-coaster {
  /* rect6 corner (10 mm + 0.2 mm inset) relative to the 93.14 mm canvas. */
  border-radius: 10.95%;
}

.sheet.is-dragging {
  cursor: grabbing;
  transition: none;
}

.sheet:focus-visible {
  outline: 2px solid var(--rs-focus);
  outline-offset: 4px;
}

.art {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
  pointer-events: none;
  transition: opacity 160ms var(--rs-ease);
}

.is-stale .art {
  opacity: 0.55;
}

.guide-margin {
  position: absolute;
  border: 1px dashed var(--rs-guide);
  opacity: 0.6;
  pointer-events: none;
}

.is-coaster .guide-margin {
  opacity: 0.35;
}

.guide-coaster {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}

.guide-coaster rect {
  stroke: var(--rs-guide);
  stroke-width: 1px;
}

.sheet-note {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  font-size: 12.5px;
  color: var(--rs-faint);
  pointer-events: none;
}

.readout {
  position: absolute;
  bottom: 10px;
  left: 50%;
  padding: 3px 8px;
  border-radius: var(--rs-radius-sm);
  background: rgba(25, 25, 24, 0.82);
  color: #fbfaf7;
  font-size: 11.5px;
  transform: translateX(-50%);
  pointer-events: none;
}

.busy {
  position: absolute;
  top: 12px;
  right: 14px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 3px 9px;
  border: 1px solid var(--rs-line);
  border-radius: 999px;
  background: rgba(255, 254, 251, 0.9);
  font-size: 12px;
  color: var(--rs-muted);
}
</style>
