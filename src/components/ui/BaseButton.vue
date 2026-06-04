<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md";
    type?: "button" | "submit" | "reset";
    disabled?: boolean;
  }>(),
  {
    variant: "secondary",
    size: "md",
    type: "button",
    disabled: false,
  },
);

const classes = computed(() => [
  "mtp-button",
  `mtp-button--${props.variant}`,
  `mtp-button--${props.size}`,
]);
</script>

<template>
  <button :type="type" :disabled="disabled" :class="classes">
    <span v-if="$slots.icon" class="mtp-button__icon" aria-hidden="true">
      <slot name="icon" />
    </span>
    <span class="mtp-button__label">
      <slot />
    </span>
  </button>
</template>

<style scoped>
.mtp-button {
  min-height: 2.5rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  font: inherit;
  font-weight: 650;
  line-height: 1;
  transition: background 160ms ease, border-color 160ms ease, color 160ms ease;
}

.mtp-button:focus-visible {
  outline: none;
  box-shadow: var(--mtp-focus-ring);
}

.mtp-button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.mtp-button--sm {
  min-height: 2rem;
  padding: 0 0.75rem;
  font-size: 0.8125rem;
}

.mtp-button--md {
  padding: 0 1rem;
  font-size: 0.875rem;
}

.mtp-button--primary {
  color: white;
  background: var(--mtp-accent);
  border-color: var(--mtp-accent);
}

.mtp-button--primary:not(:disabled):hover {
  background: var(--mtp-accent-strong);
}

.mtp-button--secondary {
  color: var(--mtp-text);
  background: var(--mtp-surface);
}

.mtp-button--secondary:not(:disabled):hover {
  background: var(--mtp-surface-2);
  border-color: var(--mtp-border-strong);
}

.mtp-button--ghost {
  color: var(--mtp-text-muted);
  background: transparent;
  border-color: transparent;
}

.mtp-button--ghost:not(:disabled):hover {
  color: var(--mtp-text);
  background: var(--mtp-surface-2);
}

.mtp-button--danger {
  color: white;
  background: var(--mtp-danger);
  border-color: var(--mtp-danger);
}

.mtp-button__icon {
  width: 1rem;
  height: 1rem;
  display: inline-flex;
}
</style>
