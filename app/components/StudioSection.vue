<script setup lang="ts">
import { ref, useId } from "vue";
import AppIcon from "./AppIcon.vue";

/** Sidebar group. Collapsible groups use a disclosure button with aria-expanded. */
const props = withDefaults(
  defineProps<{ title: string; collapsible?: boolean; defaultOpen?: boolean }>(),
  {
    collapsible: false,
    defaultOpen: true,
  },
);

const open = ref(props.collapsible ? props.defaultOpen : true);
const id = useId();
</script>

<template>
  <section class="section" :aria-labelledby="`${id}-title`">
    <div class="header">
      <h2 v-if="!collapsible" :id="`${id}-title`" class="title">{{ title }}</h2>
      <h2 v-else class="title">
        <button
          :id="`${id}-title`"
          type="button"
          class="disclosure"
          :aria-expanded="open"
          :aria-controls="`${id}-body`"
          @click="open = !open"
        >
          <AppIcon name="chevron" :size="14" class="chevron" :class="{ 'is-open': open }" />
          {{ title }}
        </button>
      </h2>
      <div v-if="$slots.actions && open" class="actions">
        <slot name="actions" />
      </div>
    </div>
    <div v-show="open" :id="`${id}-body`" class="body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.section {
  padding: 14px 16px 18px;
  border-top: 1px solid var(--rs-line);
}

.section:first-child {
  border-top: 0;
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 24px;
  margin-bottom: 10px;
}

.title {
  margin: 0;
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--rs-muted);
}

.disclosure {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin: 0 0 0 -4px;
  padding: 2px 4px;
  border: 0;
  border-radius: var(--rs-radius-sm);
  background: transparent;
  font: inherit;
  letter-spacing: inherit;
  text-transform: inherit;
  color: inherit;
}

.disclosure:hover {
  background: var(--rs-hover);
  color: var(--rs-ink);
}

.chevron {
  transform: rotate(-90deg);
  transition: transform 140ms var(--rs-ease);
}

.chevron.is-open {
  transform: none;
}

.actions {
  display: flex;
  gap: 2px;
}

.body {
  display: grid;
  gap: 14px;
}
</style>
