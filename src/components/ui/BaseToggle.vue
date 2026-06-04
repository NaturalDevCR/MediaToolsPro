<script setup lang="ts">
defineProps<{
  id: string;
  label: string;
  modelValue: boolean;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (event: "update:modelValue", value: boolean): void;
}>();
</script>

<template>
  <label class="mtp-toggle" :for="id">
    <input
      :id="id"
      class="mtp-toggle__input"
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      @change="emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
    />
    <span class="mtp-toggle__control" aria-hidden="true"></span>
    <span>{{ label }}</span>
  </label>
</template>

<style scoped>
.mtp-toggle {
  min-height: 2.75rem;
  display: inline-flex;
  align-items: center;
  gap: 0.625rem;
  color: var(--mtp-text);
  font-size: 0.875rem;
  font-weight: 600;
}

.mtp-toggle__input {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.mtp-toggle__control {
  width: 2.25rem;
  height: 1.25rem;
  border: 1px solid var(--mtp-border);
  border-radius: 999px;
  background: var(--mtp-surface-3);
  position: relative;
}

.mtp-toggle__control::after {
  content: "";
  width: 0.875rem;
  height: 0.875rem;
  border-radius: 999px;
  background: var(--mtp-text-muted);
  position: absolute;
  top: 0.125rem;
  left: 0.125rem;
  transition: transform 160ms ease, background 160ms ease;
}

.mtp-toggle__input:checked + .mtp-toggle__control {
  border-color: var(--mtp-accent);
  background: var(--mtp-accent-soft);
}

.mtp-toggle__input:checked + .mtp-toggle__control::after {
  background: var(--mtp-accent);
  transform: translateX(1rem);
}

.mtp-toggle__input:focus-visible + .mtp-toggle__control {
  box-shadow: var(--mtp-focus-ring);
}
</style>
