<script setup lang="ts">
import { computed, inject, ref, watch } from "vue";
import type { RenderMode } from "~/types/render";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";
import StudioSection from "./StudioSection.vue";
import {
  colorMoleculeGroup,
  componentGroups,
  groupRenderMode,
  groupSwatchColor,
  hasSelectedMode,
  modeMoleculeGroup,
  modeOf,
  selectMoleculeGroup,
  setComponentMode,
} from "../lib/molecule-groups";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

const DEFAULT_INK = "#191918";

const busy = computed(() => studio.status === "loading");
const chains = computed(() => studio.structure?.chains ?? []);
const chainById = computed(() => new Map(chains.value.map((c) => [c.id, c])));
const selected = computed(() => new Set(studio.options.selectedChains));
const molecules = computed(() => new Map(studio.structure?.molecules?.map((m) => [m.chainId, m])));
const groups = computed(() => (studio.structure ? componentGroups(studio.structure) : []));
const hasSlice = computed(() => hasSelectedMode(studio.options, "slice"));
const hasRibbon = computed(() => hasSelectedMode(studio.options, "ribbon"));

const expanded = ref<Record<string, boolean>>({});
watch(
  () => studio.structure,
  () => {
    expanded.value = {};
  },
);

function isExpanded(key: string, defaultExpanded: boolean): boolean {
  return expanded.value[key] ?? defaultExpanded;
}

function toggleExpanded(key: string, defaultExpanded: boolean) {
  expanded.value = {
    ...expanded.value,
    [key]: !isExpanded(key, defaultExpanded),
  };
}

function toggleGroup(ids: string[], on: boolean) {
  if (studio.structure) selectMoleculeGroup(studio.options, studio.structure, ids, on);
}

function groupCount(ids: string[]) {
  return ids.filter((id) => selected.value.has(id)).length;
}

const selectedCount = computed(
  () => chains.value.filter((chain) => selected.value.has(chain.id)).length,
);

const selectedResidues = computed(() =>
  chains.value
    .filter((chain) => selected.value.has(chain.id))
    .reduce((sum, chain) => sum + chain.residueCount, 0),
);

function toggleChain(id: string, on: boolean) {
  const next = chains.value
    .map((chain) => chain.id)
    .filter((chainId) => (chainId === id ? on : selected.value.has(chainId)));
  studio.options.selectedChains = next;
}

function selectAll() {
  studio.options.selectedChains = chains.value.map((chain) => chain.id);
}

function selectNone() {
  studio.options.selectedChains = [];
}

function colorOf(id: string): string {
  return studio.options.chainColors[id] ?? DEFAULT_INK;
}

function parentColorOf(ids: string[]): string {
  return groupSwatchColor(studio.options.chainColors, ids, DEFAULT_INK);
}

function setColor(id: string, color: string) {
  studio.options.chainColors = { ...studio.options.chainColors, [id]: color.toLowerCase() };
}

function setGroupColor(ids: string[], color: string) {
  colorMoleculeGroup(studio.options, ids, color);
}

function childModeOf(id: string): RenderMode {
  return modeOf(studio.options, id);
}

function parentModeOf(ids: string[]): RenderMode | "mixed" {
  return groupRenderMode(studio.options, ids);
}

function setChildMode(id: string, mode: RenderMode) {
  setComponentMode(studio.options, id, mode);
}

function setGroupMode(ids: string[], mode: RenderMode) {
  modeMoleculeGroup(studio.options, ids, mode);
}

function chainLabel(id: string): string {
  return molecules.value.get(id)?.label ?? (id.trim() === "" ? "(blank)" : id);
}

function countLabel(id: string): string {
  const mol = molecules.value.get(id);
  if (mol) return `${mol.atoms.length} ${mol.atoms.length === 1 ? "atom" : "atoms"}`;
  const count = chainById.value.get(id)?.residueCount ?? 0;
  return `${count.toLocaleString()} res`;
}
</script>

<template>
  <StudioSection title="Structure">
    <template v-if="!studio.structure">
      <div class="intake">
        <button
          type="button"
          class="rs-btn rs-btn--primary"
          :disabled="busy"
          @click="ui.pickFile()"
        >
          <AppIcon name="upload" :size="14" />
          Choose file…
        </button>
        <button type="button" class="rs-btn" :disabled="busy" @click="studio.loadExample()">
          <AppIcon name="flask" :size="14" />
          Example
        </button>
      </div>
      <p class="rs-hint">
        <AppIcon name="lock" :size="12" class="inline-icon" />
        PDB, ENT, CIF or mmCIF. Parsed and rendered locally in this browser — files are never
        uploaded.
      </p>
    </template>

    <template v-else>
      <div class="file">
        <AppIcon name="file" class="file-icon" />
        <div class="file-text">
          <div class="file-name" :title="studio.fileName || studio.structure.name">
            {{ studio.fileName || studio.structure.name }}
          </div>
          <div class="file-meta">
            <span class="rs-num">{{ studio.structure.residueCount.toLocaleString() }}</span>
            residues · <span class="rs-num">{{ chains.length }}</span>
            {{ chains.length === 1 ? "component" : "components" }}
          </div>
        </div>
        <button type="button" class="rs-btn rs-btn--sm" :disabled="busy" @click="ui.pickFile()">
          Replace
        </button>
      </div>

      <div v-if="chains.length" class="chains">
        <div class="chains-head">
          <span id="chains-label" class="rs-label">
            Components
            <span class="count rs-num">{{ selectedCount }}/{{ chains.length }}</span>
          </span>
          <div class="chains-actions">
            <button
              type="button"
              class="rs-btn rs-btn--ghost rs-btn--sm"
              :disabled="selectedCount === chains.length"
              @click="selectAll"
            >
              All
            </button>
            <button
              type="button"
              class="rs-btn rs-btn--ghost rs-btn--sm"
              :disabled="selectedCount === 0"
              @click="selectNone"
            >
              None
            </button>
          </div>
        </div>

        <ul class="component-tree" role="tree" aria-labelledby="chains-label">
          <li
            v-for="group in groups"
            :key="group.key"
            class="tree-group"
            role="treeitem"
            :aria-expanded="isExpanded(group.key, group.defaultExpanded)"
            :data-category="group.key"
          >
            <div class="tree-row tree-parent" :class="{ 'is-off': groupCount(group.ids) === 0 }">
              <button
                type="button"
                class="tree-toggle"
                :aria-expanded="isExpanded(group.key, group.defaultExpanded)"
                :aria-label="`${isExpanded(group.key, group.defaultExpanded) ? 'Collapse' : 'Expand'} ${group.label}`"
                @click="toggleExpanded(group.key, group.defaultExpanded)"
              >
                <AppIcon
                  name="chevron"
                  :size="12"
                  class="tree-chevron"
                  :class="{ 'is-open': isExpanded(group.key, group.defaultExpanded) }"
                />
              </button>
              <label class="row-check">
                <input
                  type="checkbox"
                  class="check"
                  :checked="groupCount(group.ids) === group.ids.length"
                  :indeterminate="
                    groupCount(group.ids) > 0 && groupCount(group.ids) < group.ids.length
                  "
                  :aria-label="`Show all ${group.label.toLowerCase()}`"
                  @change="toggleGroup(group.ids, ($event.target as HTMLInputElement).checked)"
                />
                <span class="group-title" :title="group.label">{{ group.label }}</span>
                <span class="row-count rs-num"
                  >{{ groupCount(group.ids) }}/{{ group.ids.length }}</span
                >
              </label>
              <div
                class="mode-pill"
                role="group"
                :aria-label="`Render mode for ${group.label.toLowerCase()}`"
              >
                <button
                  type="button"
                  class="mode-btn"
                  :class="{ 'is-active': parentModeOf(group.ids) === 'ribbon' }"
                  :aria-pressed="parentModeOf(group.ids) === 'ribbon'"
                  :title="`Ribbon mode for all ${group.label.toLowerCase()}`"
                  :aria-label="`Ribbon mode for all ${group.label.toLowerCase()}`"
                  @click="setGroupMode(group.ids, 'ribbon')"
                >
                  <AppIcon name="ribbon" :size="11" />
                </button>
                <button
                  type="button"
                  class="mode-btn"
                  :class="{ 'is-active': parentModeOf(group.ids) === 'slice' }"
                  :aria-pressed="parentModeOf(group.ids) === 'slice'"
                  :title="`Slicer mode for all ${group.label.toLowerCase()}`"
                  :aria-label="`Slicer mode for all ${group.label.toLowerCase()}`"
                  @click="setGroupMode(group.ids, 'slice')"
                >
                  <AppIcon name="layers" :size="11" />
                </button>
              </div>
              <label
                class="color-dot"
                :style="{ '--swatch': parentColorOf(group.ids) }"
                :title="`Choose pen color for all ${group.label.toLowerCase()}`"
              >
                <span class="rs-visually-hidden">Color all {{ group.label.toLowerCase() }}</span>
                <input
                  type="color"
                  :value="parentColorOf(group.ids)"
                  @input="setGroupColor(group.ids, ($event.target as HTMLInputElement).value)"
                />
              </label>
            </div>

            <ul
              v-show="isExpanded(group.key, group.defaultExpanded)"
              class="tree-children"
              role="group"
            >
              <li
                v-for="id in group.ids"
                :key="id"
                class="tree-row tree-child"
                role="treeitem"
                :class="{ 'is-off': !selected.has(id) }"
              >
                <label class="row-check">
                  <input
                    type="checkbox"
                    class="check"
                    :checked="selected.has(id)"
                    @change="toggleChain(id, ($event.target as HTMLInputElement).checked)"
                  />
                  <span class="chain-id rs-num" :title="chainLabel(id)">{{ chainLabel(id) }}</span>
                  <span class="row-count rs-num">{{ countLabel(id) }}</span>
                </label>
                <div
                  class="mode-pill"
                  role="group"
                  :aria-label="`Render mode for ${chainLabel(id)}`"
                >
                  <button
                    type="button"
                    class="mode-btn"
                    :class="{ 'is-active': childModeOf(id) === 'ribbon' }"
                    :aria-pressed="childModeOf(id) === 'ribbon'"
                    :title="`Ribbon mode for ${chainLabel(id)}`"
                    :aria-label="`Ribbon mode for ${chainLabel(id)}`"
                    @click="setChildMode(id, 'ribbon')"
                  >
                    <AppIcon name="ribbon" :size="11" />
                  </button>
                  <button
                    type="button"
                    class="mode-btn"
                    :class="{ 'is-active': childModeOf(id) === 'slice' }"
                    :aria-pressed="childModeOf(id) === 'slice'"
                    :title="`Slicer mode for ${chainLabel(id)}`"
                    :aria-label="`Slicer mode for ${chainLabel(id)}`"
                    @click="setChildMode(id, 'slice')"
                  >
                    <AppIcon name="layers" :size="11" />
                  </button>
                </div>
                <label
                  class="color-dot"
                  :style="{ '--swatch': colorOf(id) }"
                  :title="`Choose pen color for ${chainLabel(id)}`"
                >
                  <span class="rs-visually-hidden">Pen color for {{ chainLabel(id) }}</span>
                  <input
                    type="color"
                    :value="colorOf(id)"
                    @input="setColor(id, ($event.target as HTMLInputElement).value)"
                  />
                </label>
              </li>
            </ul>
          </li>
        </ul>

        <p v-if="selectedCount === 0" class="rs-hint warn" role="status">
          No components selected — select at least one component to draw.
        </p>
        <p v-else-if="hasSlice && hasRibbon" class="rs-hint">
          Hybrid plot: ribbon and sliced components share one 3D coordinate frame. Slicer depth uses
          multi-pass overdrawing and parallel contours — no line fading.
        </p>
        <p v-else-if="hasSlice" class="rs-hint">
          Topographic heavy-atom surface slices. Depth uses multi-pass overdrawing and parallel
          lines — no line fading. Same-color slice components share one envelope.
        </p>
        <p v-else class="rs-hint">
          {{ selectedResidues.toLocaleString() }} protein residues selected (local DSSP).
          <template v-if="molecules.size"
            >Switch any group or component between ribbon and slicer in the tree; water starts
            hidden.</template
          >
        </p>
      </div>
    </template>
  </StudioSection>
</template>

<style scoped>
.intake {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.intake .rs-btn {
  flex: 1 1 auto;
}

.inline-icon {
  margin: -2px 3px 0 0;
}

.file {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  padding: 8px 8px 8px 10px;
  border: 1px solid var(--rs-line);
  border-radius: var(--rs-radius);
  background: var(--rs-surface);
}

.file-icon {
  flex: none;
  color: var(--rs-muted);
}

.file .rs-btn {
  flex: none;
}

.file-text {
  flex: 1 1 0;
  min-width: 0;
}

.file-name {
  overflow: hidden;
  font-weight: 500;
  font-size: 13px;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--rs-ink-strong);
}

.file-meta {
  overflow: hidden;
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: var(--rs-muted);
}

.chains {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 6px;
  min-width: 0;
}

.chains-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  min-width: 0;
}

.count {
  margin-left: 4px;
  font-weight: 400;
  color: var(--rs-muted);
}

.chains-actions {
  display: flex;
  flex: none;
  gap: 2px;
}

.component-tree {
  min-width: 0;
  max-height: 272px;
  margin: 0;
  padding: 3px;
  overflow-x: hidden;
  overflow-y: auto;
  list-style: none;
  border: 1px solid var(--rs-line);
  border-radius: var(--rs-radius);
  background: var(--rs-surface);
}

.tree-group + .tree-group {
  margin-top: 2px;
}

.tree-row {
  display: flex;
  align-items: center;
  gap: 3px;
  min-width: 0;
  padding: 0 2px;
  border-radius: var(--rs-radius-sm);
}

.tree-row:hover {
  background: var(--rs-hover);
}

.tree-toggle {
  display: inline-grid;
  place-items: center;
  flex: none;
  width: 18px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: var(--rs-radius-sm);
  background: transparent;
  color: var(--rs-muted);
  cursor: pointer;
}

.tree-toggle:hover {
  background: var(--rs-hover);
  color: var(--rs-ink-strong);
}

.tree-chevron {
  transform: rotate(-90deg);
  transition: transform 140ms var(--rs-ease);
}

.tree-chevron.is-open {
  transform: none;
}

.tree-children {
  margin: 0 0 2px 10px;
  padding: 0 0 0 8px;
  list-style: none;
  border-left: 1px solid var(--rs-line);
}

.tree-child {
  padding-left: 3px;
}

.row-check {
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  gap: 6px;
  min-width: 0;
  min-height: 28px;
  padding: 0 2px;
  cursor: pointer;
}

.check {
  flex: none;
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--rs-ink-strong);
  cursor: pointer;
}

.group-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 600;
  color: var(--rs-ink-strong);
}

.chain-id {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  font-weight: 500;
  color: var(--rs-ink-strong);
}

.row-count {
  flex: none;
  margin-left: auto;
  padding-left: 4px;
  font-size: 11px;
  color: var(--rs-muted);
  white-space: nowrap;
}

.is-off .group-title,
.is-off .chain-id,
.is-off .row-count {
  color: var(--rs-faint);
}

.mode-pill {
  display: inline-flex;
  flex: none;
  gap: 1px;
  padding: 1px;
  border-radius: var(--rs-radius-sm);
  background: rgba(55, 53, 47, 0.06);
}

.mode-btn {
  display: inline-grid;
  place-items: center;
  width: 20px;
  height: 19px;
  padding: 0;
  border: 0;
  border-radius: 3px;
  background: transparent;
  color: var(--rs-muted);
  cursor: pointer;
  transition:
    background-color 120ms var(--rs-ease),
    color 120ms var(--rs-ease);
}

.mode-btn:hover {
  color: var(--rs-ink-strong);
}

.mode-btn.is-active {
  background: var(--rs-surface);
  color: var(--rs-ink-strong);
  box-shadow: 0 0 0 1px var(--rs-line);
}

.mode-btn:focus-visible {
  outline: 2px solid var(--rs-focus);
  outline-offset: 1px;
}

.is-off .mode-pill {
  opacity: 0.5;
}

.color-dot {
  position: relative;
  display: inline-block;
  flex: none;
  width: 22px;
  height: 22px;
  border-radius: var(--rs-radius-sm);
  cursor: pointer;
}

.color-dot::after {
  position: absolute;
  inset: 5px;
  border-radius: 50%;
  background: var(--swatch);
  box-shadow: 0 0 0 1px rgba(15, 15, 15, 0.16);
  content: "";
}

.color-dot:hover {
  background: var(--rs-hover);
}

.is-off .color-dot::after {
  opacity: 0.35;
}

.color-dot input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  padding: 0;
  border: 0;
  opacity: 0;
  cursor: pointer;
}

.color-dot:has(input:focus-visible) {
  outline: 2px solid var(--rs-focus);
  outline-offset: 1px;
}

.warn {
  color: var(--rs-warn);
}
</style>
