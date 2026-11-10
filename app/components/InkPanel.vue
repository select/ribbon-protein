<script setup lang="ts">
import { computed, inject } from "vue";
import type { Studio } from "../app.vue";
import { hasSelectedMode, modeOf, moleculeGroups } from "../lib/molecule-groups";
import DrawingGroup from "./DrawingGroup.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";
import ToggleSwitch from "./ToggleSwitch.vue";

const studio = inject<Studio>("ribbon")!;
const hasSlice = computed(() => hasSelectedMode(studio.options, "slice"));
const hasRibbon = computed(
  () => !studio.structure || hasSelectedMode(studio.options, "ribbon") || !hasSlice.value,
);
const hasRibbonProtein = computed(() => {
  if (!studio.structure) return true;
  const molIds = new Set((studio.structure.molecules ?? []).map((m) => m.chainId));
  const proteinIds = studio.structure.chains.filter((c) => !molIds.has(c.id)).map((c) => c.id);
  if (!proteinIds.length) return false;
  return proteinIds.some((id) => modeOf(studio.options, id) === "ribbon");
});
const groups = computed(() => {
  const molecules = studio.structure?.molecules ?? [];
  return moleculeGroups(molecules).filter((group) =>
    molecules.some(
      (m) =>
        m.category === group.category &&
        m.bonds.length > 0 &&
        modeOf(studio.options, m.chainId) === "ribbon",
    ),
  );
});
const passOptions = [
  { value: 1, label: "1" },
  { value: 2, label: "2" },
  { value: 3, label: "3" },
];
</script>

<template>
  <StudioSection title="Ink" data-test="ink-panel">
    <RangeField
      v-model="studio.options.penWidthMm"
      :label="hasSlice && !hasRibbon ? 'Pen width' : 'Default pen width'"
      :min="0.1"
      :max="2"
      :step="0.05"
      unit="mm"
      :hint="
        hasSlice && !hasRibbon
          ? 'Single physical pen width. Translucent ink simulates darkening where strokes stack.'
          : 'Stroke width for proteins, slices and non-lipid molecules. Lipids have their own stroke setting.'
      "
    />

    <template v-if="hasSlice">
      <DrawingGroup title="Surface slicing" default-open data-category="slice-surface">
        <RangeField
          v-model="studio.options.sliceCount"
          label="Slices"
          :min="2"
          :max="120"
          :step="1"
          hint="Number of section planes cut through sliced components."
        />
        <RangeField
          v-model="studio.options.sliceCoarseness"
          label="Coarseness"
          :min="0"
          :max="100"
          :step="1"
          unit="%"
          hint="Smooths individual atomic bumps into broad topographic sheets."
        />
        <RangeField
          v-model="studio.options.sliceProbeAngstrom"
          label="Envelope probe"
          :min="0"
          :max="3.5"
          :step="0.1"
          unit="Å"
          hint="Expands heavy-atom radii toward a solvent-accessible envelope."
        />
      </DrawingGroup>
      <DrawingGroup title="Depth and overdrawing" default-open data-category="slice-depth">
        <RangeField
          v-model="studio.options.sliceMaxPasses"
          label="Overdraw passes"
          :min="1"
          :max="8"
          :step="1"
          hint="Foreground contours are retraced up to this many times to build physical ink density."
        />
        <RangeField
          v-model="studio.options.sliceMaxParallel"
          label="Parallel lines"
          :min="1"
          :max="7"
          :step="1"
          hint="Flanking parallel contour tracks added on closer slices."
        />
        <RangeField
          v-model="studio.options.sliceParallelSpacingMm"
          label="Parallel spacing"
          :min="0.05"
          :max="1"
          :step="0.01"
          unit="mm"
          :disabled="studio.options.sliceMaxParallel < 2"
          hint="Offset between adjacent parallel contour tracks on paper."
        />
        <RangeField
          v-model="studio.options.sliceDepthGamma"
          label="Depth contrast"
          :min="0.4"
          :max="3.2"
          :step="0.1"
          hint="Concentrates extra passes and parallel lines on the nearest contours."
        />
      </DrawingGroup>
    </template>

    <DrawingGroup
      v-if="hasRibbon && hasRibbonProtein"
      title="Protein drawing"
      :default-open="!hasSlice"
      data-category="protein"
    >
      <RangeField
        v-model="studio.options.lines"
        label="Protein interior lines"
        :min="0"
        :max="20"
        :step="1"
        hint="Contour lines inside each helix and sheet ribbon."
      />
      <DrawingGroup title="Loop and border settings">
        <ToggleSwitch
          v-model="studio.options.fillLoops"
          label="Fill loops"
          hint="Draw unstructured loop tubes with close parallel paths."
        />
        <RangeField
          v-model="studio.options.loopSpacingMm"
          label="Loop spacing"
          :min="0.02"
          :max="1"
          :step="0.01"
          unit="mm"
          :disabled="!studio.options.fillLoops"
        />
        <SegmentedControl
          v-model="studio.options.borderPasses"
          legend="Border passes"
          :options="passOptions"
        />
        <RangeField
          v-model="studio.options.borderSpacingMm"
          label="Border spacing"
          :min="0.01"
          :max="0.5"
          :step="0.01"
          unit="mm"
          :disabled="studio.options.borderPasses < 2"
          hint="Inset between parallel helix and sheet outlines."
        />
      </DrawingGroup>
    </DrawingGroup>

    <DrawingGroup
      v-for="group in groups"
      :key="group.category"
      :title="`${group.label} drawing`"
      :default-open="group.category === 'lipid' && !hasSlice"
      :data-category="group.category"
    >
      <RangeField
        v-model="studio.options.moleculeStyles[group.category].lines"
        :label="`${group.label} interior lines`"
        :min="0"
        :max="20"
        :step="1"
      />
      <RangeField
        v-model="studio.options.moleculeStyles[group.category].bondWidthAngstrom"
        :label="`${group.label} bond width`"
        unit="Å"
        :min="0"
        :max="4"
        :step="0.1"
        hint="Artistic band thickness. Set to 0 for single bond lines."
      />
      <RangeField
        v-if="group.category === 'lipid'"
        v-model="studio.options.lipidWidthMm"
        label="Lipid stroke"
        unit="mm"
        :min="0.1"
        :max="2"
        :step="0.05"
      />
    </DrawingGroup>

    <p v-if="hasSlice" class="rs-hint">
      Designed for pen plotters: no strokes are faded. Slicer depth comes from repeated passes and
      parallel contour tracks.
    </p>
    <p v-else-if="groups.length" class="rs-hint">
      Each group has independent interior lines. Bond bands have two outlines plus these interior
      tracks; isolated atoms stay circles. The physical pen determines actual ink width.
    </p>
  </StudioSection>
</template>
