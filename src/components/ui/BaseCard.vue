<script setup lang="ts">
withDefaults(
  defineProps<{
    as?: "section" | "article" | "div";
  }>(),
  {
    as: "section",
  },
);
</script>

<template>
  <component :is="as" class="mtp-card">
    <slot />
  </component>
</template>

<style scoped>
.mtp-card {
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  background: var(--mtp-surface);
  color: var(--mtp-text);
  box-shadow: var(--mtp-shadow-sm);
}
</style>
