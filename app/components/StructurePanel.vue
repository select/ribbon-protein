<script setup lang="ts">
import { computed, inject } from "vue";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";
import StudioSection from "./StudioSection.vue";
import { moleculeGroups, selectMoleculeGroup, colorMoleculeGroup } from "../lib/molecule-groups";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

const DEFAULT_INK = "#191918";
const PEN_SWATCHES = ["#191918", "#c4302b", "#1f5fbf", "#2f7d4a"];

const busy = computed(() => studio.status === "loading");
const chains = computed(() => studio.structure?.chains ?? []);
const selected = computed(() => new Set(studio.options.selectedChains));
const molecules = computed(() => new Map(studio.structure?.molecules?.map((m) => [m.chainId, m])));
const groups = computed(() => moleculeGroups(studio.structure?.molecules ?? []));
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

function setColor(id: string, color: string) {
  studio.options.chainColors = { ...studio.options.chainColors, [id]: color.toLowerCase() };
}

function clearColor(id: string) {
  const next = { ...studio.options.chainColors };
  delete next[id];
  studio.options.chainColors = next;
}

function cycleSwatch(id: string) {
  const current = colorOf(id).toLowerCase();
  const index = PEN_SWATCHES.indexOf(current);
  const color = PEN_SWATCHES[(index + 1) % PEN_SWATCHES.length] ?? DEFAULT_INK;
  if (color === DEFAULT_INK) clearColor(id);
  else setColor(id, color);
}

function chainLabel(id: string): string {
  return molecules.value.get(id)?.label ?? (id.trim() === "" ? "(blank)" : id);
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

        <div v-if="groups.length" class="molecule-categories">
          <div
            v-for="group in groups"
            :key="group.category"
            class="molecule-category"
            :data-category="group.category"
          >
            <div class="molecule-group">
              <label>
                <input
                  type="checkbox"
                  :checked="groupCount(group.ids) === group.ids.length"
                  :indeterminate="
                    groupCount(group.ids) > 0 && groupCount(group.ids) < group.ids.length
                  "
                  :aria-label="`Show all ${group.label.toLowerCase()}`"
                  @change="toggleGroup(group.ids, ($event.target as HTMLInputElement).checked)"
                />
                {{ group.label }}
                <span class="count rs-num">{{ groupCount(group.ids) }}/{{ group.ids.length }}</span>
              </label>
              <input
                type="color"
                :value="colorOf(group.ids[0]!)"
                :aria-label="`Color all ${group.label.toLowerCase()}`"
                :title="`Apply color to all ${group.label.toLowerCase()}, including hidden components`"
                @input="
                  colorMoleculeGroup(
                    studio.options,
                    group.ids,
                    ($event.target as HTMLInputElement).value,
                  )
                "
              />
            </div>
          </div>
        </div>
        <ul class="chain-list" aria-labelledby="chains-label">
          <li
            v-for="chain in chains"
            :key="chain.id"
            class="chain"
            :class="{ 'is-off': !selected.has(chain.id) }"
          >
            <label class="chain-check">
              <input
                type="checkbox"
                class="check"
                :checked="selected.has(chain.id)"
                @change="toggleChain(chain.id, ($event.target as HTMLInputElement).checked)"
              />
              <span class="chain-id rs-num" :title="chainLabel(chain.id)">{{
                chainLabel(chain.id)
              }}</span>
              <span class="chain-count">{{
                molecules.get(chain.id)
                  ? `${molecules.get(chain.id)!.atoms.length} atoms`
                  : `${chain.residueCount.toLocaleString()} res`
              }}</span>
            </label>
            <div class="chain-color">
              <button
                type="button"
                class="swatch-cycle"
                :style="{ '--swatch': colorOf(chain.id) }"
                :title="`Cycle pen color for chain ${chainLabel(chain.id)}`"
                :aria-label="`Cycle preset pen color for chain ${chainLabel(chain.id)}`"
                @click="cycleSwatch(chain.id)"
              />
              <label class="picker" :title="`Custom pen color for chain ${chainLabel(chain.id)}`">
                <span class="rs-visually-hidden"
                  >Custom pen color for chain {{ chainLabel(chain.id) }}</span
                >
                <input
                  type="color"
                  :value="colorOf(chain.id)"
                  @input="setColor(chain.id, ($event.target as HTMLInputElement).value)"
                />
                <AppIcon name="pen" :size="12" />
              </label>
            </div>
          </li>
        </ul>

        <p v-if="selectedCount === 0" class="rs-hint warn" role="status">
          No components selected — select at least one component to draw.
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
.molecule-categories {
  display: grid;
  gap: 6px;
  font-size: 12px;
}
.molecule-group {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.molecule-group label {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
}
.molecule-group input[type="color"] {
  width: 28px;
  height: 24px;
  padding: 1px;
  border: 1px solid var(--rs-line);
  border-radius: 4px;
  cursor: pointer;
}

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

.chain-list {
  max-height: 232px;
  margin: 0;
  padding: 2px;
  overflow-y: auto;
  list-style: none;
  border: 1px solid var(--rs-line);
  border-radius: var(--rs-radius);
  background: var(--rs-surface);
}

.chain {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 4px 0 0;
  border-radius: var(--rs-radius-sm);
}

.chain:hover {
  background: var(--rs-hover);
}

.chain-check {
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  gap: 8px;
  min-width: 0;
  min-height: 28px;
  padding: 0 6px;
  cursor: pointer;
}

.check {
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--rs-ink-strong);
  cursor: pointer;
}

.chain-id {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
  color: var(--rs-ink-strong);
}

.chain-count {
  margin-left: auto;
  font-size: 12px;
  color: var(--rs-muted);
  white-space: nowrap;
}

.is-off .chain-id,
.is-off .chain-count {
  color: var(--rs-faint);
}

.chain-color {
  display: flex;
  align-items: center;
  gap: 2px;
}

.swatch-cycle {
  width: 22px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: var(--rs-radius-sm);
  background: transparent;
  position: relative;
}

.swatch-cycle::after {
  position: absolute;
  inset: 6px;
  border-radius: 50%;
  background: var(--swatch);
  box-shadow: 0 0 0 1px rgba(15, 15, 15, 0.12);
  content: "";
}

.swatch-cycle:hover {
  background: var(--rs-hover);
}

.is-off .swatch-cycle::after {
  opacity: 0.35;
}

.picker {
  position: relative;
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: var(--rs-radius-sm);
  color: var(--rs-faint);
  cursor: pointer;
}

.picker:hover {
  background: var(--rs-hover);
  color: var(--rs-ink);
}

.picker input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  padding: 0;
  border: 0;
  opacity: 0;
  cursor: pointer;
}

.picker:has(input:focus-visible) {
  outline: 2px solid var(--rs-focus);
  outline-offset: 1px;
}

.warn {
  color: var(--rs-warn);
}
</style>
