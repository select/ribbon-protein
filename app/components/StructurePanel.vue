<script setup lang="ts">
import { computed, inject, ref, watch } from "vue";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";
import StudioSection from "./StudioSection.vue";
import {
  colorMoleculeGroup,
  componentGroups,
  groupSwatchColor,
  selectMoleculeGroup,
} from "../lib/molecule-groups";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

const DEFAULT_INK = "#191918";

const busy = computed(() => studio.status === "loading");
const isSlice = computed(() => studio.options.renderMode === "slice");
const chains = computed(() => studio.structure?.chains ?? []);
const chainById = computed(() => new Map(chains.value.map((c) => [c.id, c])));
const selected = computed(() => new Set(studio.options.selectedChains));
const molecules = computed(() => new Map(studio.structure?.molecules?.map((m) => [m.chainId, m])));
const groups = computed(() => (studio.structure ? componentGroups(studio.structure) : []));

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
                <span class="group-title">{{ group.label }}</span>
                <span class="row-count rs-num"
                  >{{ groupCount(group.ids) }}/{{ group.ids.length }}</span
                >
              </label>
              <label
                v-if="!isSlice"
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
                <label
                  v-if="!isSlice"
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
        <p v-else-if="isSlice" class="rs-hint">
          Selected components form a unified heavy-atom surface envelope plotted in black. Water is
          hidden initially; hydrogens are omitted.
        </p>
        <p v-else class="rs-hint">
          {{ selectedResidues.toLocaleString() }} protein residues selected (local DSSP).
          <template v-if="molecules.size"
            >Non-proteins: heavy-atom bond bands and isolated-atom circles. Water is hidden
            initially; hydrogens are omitted.</template
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
  padding: 8px 8px 8px 10px;
  border: 1px solid var(--rs-line);
  border-radius: var(--rs-radius);
  background: var(--rs-surface);
}

.file-icon {
  color: var(--rs-muted);
}

.file-text {
  flex: 1 1 auto;
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
  font-size: 12px;
  color: var(--rs-muted);
}

.chains {
  display: grid;
  gap: 6px;
}

.chains-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.count {
  margin-left: 4px;
  font-weight: 400;
  color: var(--rs-muted);
}

.chains-actions {
  display: flex;
  gap: 2px;
}

.component-tree {
  max-height: 264px;
  margin: 0;
  padding: 3px;
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
  gap: 4px;
  padding: 0 4px 0 2px;
  border-radius: var(--rs-radius-sm);
}

.tree-row:hover {
  background: var(--rs-hover);
}

.tree-toggle {
  display: inline-grid;
  place-items: center;
  flex: none;
  width: 20px;
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
  margin: 0 0 2px 11px;
  padding: 0 0 0 9px;
  list-style: none;
  border-left: 1px solid var(--rs-line);
}

.tree-child {
  padding-left: 4px;
}

.row-check {
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  gap: 7px;
  min-width: 0;
  min-height: 27px;
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
  font-size: 12.5px;
  font-weight: 600;
  color: var(--rs-ink-strong);
}

.chain-id {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--rs-ink-strong);
}

.row-count {
  margin-left: auto;
  padding-left: 6px;
  font-size: 11.5px;
  color: var(--rs-muted);
  white-space: nowrap;
}

.is-off .group-title,
.is-off .chain-id,
.is-off .row-count {
  color: var(--rs-faint);
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
