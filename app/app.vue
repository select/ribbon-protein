<script lang="ts">
import type { UnwrapNestedRefs } from "vue";
import type { useRibbon } from "~/composables/useRibbon";

/** Reactive (ref-unwrapped) view of useRibbon(), provided as "ribbon". */
export type Studio = UnwrapNestedRefs<ReturnType<typeof useRibbon>>;

/** Local presentation state, provided as "ribbon-ui". Never sent to the renderer. */
export interface StudioUi {
  showGuides: boolean;
  drawerOpen: boolean;
  isDragging: boolean;
  pickFile: () => void;
}
</script>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, provide, reactive, ref, watch } from "vue";
import { useRibbon as createRibbon } from "~/composables/useRibbon";
import AdvancedPanel from "~/components/AdvancedPanel.vue";
import AppIcon from "~/components/AppIcon.vue";
import ErrorBanner from "~/components/ErrorBanner.vue";
import InkPanel from "~/components/InkPanel.vue";
import PaperPanel from "~/components/PaperPanel.vue";
import PreviewStage from "~/components/PreviewStage.vue";
import StatsBar from "~/components/StatsBar.vue";
import StructurePanel from "~/components/StructurePanel.vue";
import StudioToolbar from "~/components/StudioToolbar.vue";
import ViewPanel from "~/components/ViewPanel.vue";

const studio: Studio = reactive(createRibbon());

const fileInput = ref<HTMLInputElement | null>(null);
const sidebar = ref<HTMLElement | null>(null);
const isMobile = ref(false);
let mobileQuery: MediaQueryList | undefined;

function updateMobileLayout() {
  isMobile.value = mobileQuery?.matches ?? false;
  if (!isMobile.value) ui.drawerOpen = false;
}

const ui = reactive<StudioUi>({
  showGuides: true,
  drawerOpen: false,
  isDragging: false,
  pickFile: () => fileInput.value?.click(),
});

provide("ribbon", studio);
provide("ribbon-ui", ui);

const ACCEPT = ".cif,.mmcif,.pdb,.ent";

function openFile(file: File | undefined | null) {
  if (!file) return;
  ui.drawerOpen = false;
  void studio.loadFile(file);
}

function onFileChosen(event: Event) {
  const input = event.target as HTMLInputElement;
  openFile(input.files?.[0]);
  // Allow choosing the same file again after an error.
  input.value = "";
}

/* ------------------------------------------------------ window drag & drop */

let dragDepth = 0;

function hasFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes("Files");
}

function onDragEnter(event: DragEvent) {
  if (!hasFiles(event)) return;
  event.preventDefault();
  dragDepth += 1;
  ui.isDragging = true;
}

function onDragOver(event: DragEvent) {
  if (!hasFiles(event)) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
}

function onDragLeave(event: DragEvent) {
  if (!hasFiles(event)) return;
  dragDepth = Math.max(0, dragDepth - 1);
  if (dragDepth === 0) ui.isDragging = false;
}

function onDrop(event: DragEvent) {
  if (!hasFiles(event)) return;
  event.preventDefault();
  dragDepth = 0;
  ui.isDragging = false;
  openFile(event.dataTransfer?.files?.[0]);
}

/* ------------------------------------------------------------ mobile drawer */

function onKeydown(event: KeyboardEvent) {
  if (!isMobile.value || !ui.drawerOpen) return;
  if (event.key === "Escape") {
    event.preventDefault();
    ui.drawerOpen = false;
  } else if (event.key === "Tab") {
    const controls = Array.from(
      sidebar.value?.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), a[href], [tabindex="0"]',
      ) ?? [],
    ).filter((element) => element.getClientRects().length > 0);
    const first = controls[0];
    const last = controls.at(-1);
    if (
      event.shiftKey &&
      (document.activeElement === first || !sidebar.value?.contains(document.activeElement))
    ) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
}

watch(
  () => ui.drawerOpen,
  async (open) => {
    await nextTick();
    if (open) {
      sidebar.value
        ?.querySelector<HTMLElement>("button:not(:disabled), input, [tabindex]")
        ?.focus();
      return;
    }
    // Return focus to the drawer toggle when it is visible (mobile layout) and focus was
    // inside the closing drawer or got lost to <body> (e.g. a button became disabled).
    const menu = document.querySelector<HTMLElement>(".toolbar .menu");
    const active = document.activeElement;
    const lost = !active || active === document.body || sidebar.value?.contains(active);
    if (menu && menu.offsetParent !== null && lost) menu.focus();
  },
);

onMounted(() => {
  mobileQuery = window.matchMedia("(max-width: 900px)");
  updateMobileLayout();
  mobileQuery.addEventListener("change", updateMobileLayout);
  window.addEventListener("dragenter", onDragEnter);
  window.addEventListener("dragover", onDragOver);
  window.addEventListener("dragleave", onDragLeave);
  window.addEventListener("drop", onDrop);
  window.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  mobileQuery?.removeEventListener("change", updateMobileLayout);
  window.removeEventListener("dragenter", onDragEnter);
  window.removeEventListener("dragover", onDragOver);
  window.removeEventListener("dragleave", onDragLeave);
  window.removeEventListener("drop", onDrop);
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <div class="studio" :class="{ 'drawer-open': ui.drawerOpen }">
    <a class="skip" href="#studio-preview">Skip to preview</a>

    <aside
      id="studio-settings"
      ref="sidebar"
      class="sidebar"
      aria-label="Render settings"
      :role="isMobile && ui.drawerOpen ? 'dialog' : undefined"
      :aria-modal="isMobile && ui.drawerOpen ? true : undefined"
      :inert="isMobile && !ui.drawerOpen"
    >
      <div class="brand">
        <span class="mark" aria-hidden="true"
          ><AppIcon name="ribbon" :size="16" :stroke-width="1.3"
        /></span>
        <span class="brand-name">Ribbon Studio</span>
        <button
          type="button"
          class="rs-btn rs-btn--ghost rs-btn--icon rs-btn--sm close"
          aria-label="Close settings"
          @click="ui.drawerOpen = false"
        >
          <AppIcon name="close" :size="14" />
        </button>
      </div>

      <div class="sidebar-scroll">
        <StructurePanel />
        <PaperPanel />
        <ViewPanel />
        <InkPanel />
        <AdvancedPanel />
      </div>

      <p class="sidebar-foot">
        <AppIcon name="lock" :size="12" />
        Runs entirely in your browser. Structures are never uploaded.
      </p>
    </aside>

    <div v-if="ui.drawerOpen" class="backdrop" aria-hidden="true" @click="ui.drawerOpen = false" />

    <main id="studio-preview" class="workspace" tabindex="-1" :inert="isMobile && ui.drawerOpen">
      <StudioToolbar />
      <ErrorBanner />
      <PreviewStage />
      <StatsBar />
    </main>

    <div v-if="ui.isDragging" class="drop-overlay" aria-hidden="true">
      <div class="drop-overlay-inner">
        <AppIcon name="upload" :size="20" />
        Drop to open locally
      </div>
    </div>

    <input
      ref="fileInput"
      class="rs-visually-hidden"
      type="file"
      :accept="ACCEPT"
      tabindex="-1"
      aria-hidden="true"
      @change="onFileChosen"
    />
  </div>
</template>

<style scoped>
.studio {
  display: grid;
  grid-template-columns: var(--rs-sidebar-w) minmax(0, 1fr);
  height: 100vh;
  height: 100dvh;
  overflow: hidden;
}

.skip {
  position: absolute;
  top: 8px;
  left: 8px;
  z-index: 100;
  padding: 6px 10px;
  border-radius: var(--rs-radius);
  background: var(--rs-ink-strong);
  color: #fff;
  font-size: 13px;
  text-decoration: none;
  transform: translateY(-200%);
}

.skip:focus-visible {
  transform: none;
}

/* ----------------------------------------------------------------- sidebar */

.sidebar {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-right: 1px solid var(--rs-line);
  background: var(--rs-sidebar);
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 48px;
  padding: 0 12px 0 16px;
  border-bottom: 1px solid var(--rs-line);
  flex: none;
}

.mark {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: 1px solid var(--rs-line-strong);
  border-radius: 5px;
  background: var(--rs-paper);
  color: var(--rs-ink-strong);
}

.brand-name {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--rs-ink-strong);
}

.close {
  display: none;
  margin-left: auto;
}

.sidebar-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-width: thin;
  scrollbar-color: rgba(55, 53, 47, 0.2) transparent;
}

.sidebar-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  padding: 10px 16px;
  border-top: 1px solid var(--rs-line);
  font-size: 11.5px;
  color: var(--rs-muted);
  flex: none;
}

/* --------------------------------------------------------------- workspace */

.workspace {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--rs-workspace);
}

.workspace:focus {
  outline: none;
}

/* ------------------------------------------------------------ drop overlay */

.drop-overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: grid;
  place-items: center;
  background: rgba(239, 237, 232, 0.72);
  pointer-events: none;
}

.drop-overlay-inner {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 18px 26px;
  border: 1px dashed var(--rs-ink);
  border-radius: 10px;
  background: var(--rs-paper);
  font-size: 14px;
  font-weight: 500;
  color: var(--rs-ink-strong);
}

/* ------------------------------------------------------------------ mobile */

.backdrop {
  display: none;
}

@media (max-width: 900px) {
  .studio {
    grid-template-columns: minmax(0, 1fr);
  }

  .sidebar {
    position: fixed;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 40;
    width: min(var(--rs-sidebar-w), 88vw);
    box-shadow: 0 0 32px -8px rgba(15, 15, 15, 0.25);
    transform: translateX(-100%);
    visibility: hidden;
    transition:
      transform 200ms var(--rs-ease),
      visibility 0s linear 200ms;
  }

  .drawer-open .sidebar {
    transform: none;
    visibility: visible;
    transition:
      transform 200ms var(--rs-ease),
      visibility 0s;
  }

  .close {
    display: inline-flex;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: block;
    background: rgba(25, 25, 24, 0.24);
  }
}
</style>
