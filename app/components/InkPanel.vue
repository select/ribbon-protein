<script setup lang="ts">
import { computed, inject } from "vue";
import type { Studio } from "../app.vue";
import { moleculeGroups } from "../lib/molecule-groups";
import DrawingGroup from "./DrawingGroup.vue";
import RangeField from "./RangeField.vue";
import SegmentedControl from "./SegmentedControl.vue";
import StudioSection from "./StudioSection.vue";
import ToggleSwitch from "./ToggleSwitch.vue";

const studio = inject<Studio>("ribbon")!;
const groups = computed(() => {
  const molecules = studio.structure?.molecules ?? [];
  return moleculeGroups(molecules).filter((group) =>
    molecules.some((m) => m.category === group.category && m.bonds.length > 0),
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
      label="Default pen width"
      :min="0.1"
      :max="2"
      :step="0.05"
      unit="mm"
      hint="Stroke width for proteins and non-lipid molecules. Lipids have their own stroke setting."
    />
    <DrawingGroup title="Protein drawing" default-open data-category="protein">
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
      :default-open="group.category === 'lipid'"
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
    <p v-if="groups.length" class="rs-hint">
      Each group has independent interior lines. Bond bands have two outlines plus these interior
      tracks; isolated atoms stay circles. The physical pen determines actual ink width.
    </p>
  </StudioSection>
</template>
