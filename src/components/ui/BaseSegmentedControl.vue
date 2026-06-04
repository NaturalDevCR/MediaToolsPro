<script setup lang="ts">
export type SegmentedOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

defineProps<{
  modelValue: string;
  options: SegmentedOption[];
  label: string;
}>();

const emit = defineEmits<{
  (event: "update:modelValue", value: string): void;
}>();
</script>

<template>
  <div class="mtp-segmented" role="group" :aria-label="label">
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      class="mtp-segmented__option"
      :class="{ 'is-active': option.value === modelValue }"
      :disabled="option.disabled"
      :aria-pressed="option.value === modelValue"
      @click="emit('update:modelValue', option.value)"
    >
      {{ option.label }}
    </button>
  </div>
</template>

<style scoped>
.mtp-segmented {
  display: inline-grid;
  grid-auto-flow: column;
  gap: 0.25rem;
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  background: var(--mtp-surface-2);
  padding: 0.25rem;
}

.mtp-segmented__option {
  min-height: 2rem;
  border: 0;
  border-radius: var(--mtp-radius-sm);
  background: transparent;
  color: var(--mtp-text-muted);
  font: inherit;
  font-size: 0.875rem;
  font-weight: 650;
  padding: 0 0.75rem;
}

.mtp-segmented__option.is-active {
  background: var(--mtp-surface);
  color: var(--mtp-text);
  box-shadow: var(--mtp-shadow-sm);
}

.mtp-segmented__option:focus-visible {
  outline: none;
  box-shadow: var(--mtp-focus-ring);
}
</style>
