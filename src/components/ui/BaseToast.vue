<script setup lang="ts">
withDefaults(
  defineProps<{
    tone?: "info" | "success" | "warning" | "danger";
  }>(),
  {
    tone: "info",
  },
);
</script>

<template>
  <div class="mtp-toast" :class="`mtp-toast--${tone}`" role="status">
    <slot />
  </div>
</template>

<style scoped>
.mtp-toast {
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  background: var(--mtp-surface);
  color: var(--mtp-text);
  padding: 0.75rem 1rem;
  font-size: 0.875rem;
  box-shadow: var(--mtp-shadow-md);
}

.mtp-toast--success {
  border-color: var(--mtp-success);
  background: var(--mtp-success-soft);
}

.mtp-toast--warning {
  border-color: var(--mtp-warning);
  background: var(--mtp-warning-soft);
}

.mtp-toast--danger {
  border-color: var(--mtp-danger);
  background: var(--mtp-danger-soft);
}
</style>
