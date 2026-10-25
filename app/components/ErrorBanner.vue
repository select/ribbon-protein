<script setup lang="ts">
import { computed, inject } from "vue";
import type { Studio, StudioUi } from "../app.vue";
import AppIcon from "./AppIcon.vue";

const studio = inject<Studio>("ribbon")!;
const ui = inject<StudioUi>("ribbon-ui")!;

const canRetry = computed(() => studio.structure !== null);
</script>

<template>
  <div v-if="studio.error" class="banner" role="alert">
    <AppIcon name="alert" class="icon" />
    <p class="message">{{ studio.error }}</p>
    <div class="actions">
      <button v-if="canRetry" type="button" class="rs-btn rs-btn--sm" @click="studio.render()">
        <AppIcon name="reset" :size="12" />
        Retry render
      </button>
      <button type="button" class="rs-btn rs-btn--sm" @click="ui.pickFile()">
        Choose another file
      </button>
      <button
        v-if="!canRetry"
        type="button"
        class="rs-btn rs-btn--ghost rs-btn--sm"
        @click="studio.loadExample()"
      >
        Load example
      </button>
    </div>
  </div>
</template>

<style scoped>
.banner {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: 8px 10px;
  margin: 12px 16px 0;
  padding: 9px 10px 9px 12px;
  border: 1px solid rgba(196, 85, 77, 0.25);
  border-radius: var(--rs-radius);
  background: var(--rs-danger-bg);
  color: var(--rs-ink);
}

.icon {
  margin-top: 2px;
  color: var(--rs-danger);
}

.message {
  flex: 1 1 240px;
  min-width: 0;
  margin: 0;
  font-size: 13px;
  overflow-wrap: anywhere;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

@media (max-width: 900px) {
  .banner {
    margin: 10px 10px 0;
  }
}
</style>
