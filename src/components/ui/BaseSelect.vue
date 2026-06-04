<script setup lang="ts">
export type SelectOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

defineProps<{
  id?: string;
  name?: string;
  modelValue: string;
  options: SelectOption[];
  disabled?: boolean;
  ariaDescribedby?: string;
}>();

const emit = defineEmits<{
  (event: "update:modelValue", value: string): void;
}>();
</script>

<template>
  <select
    class="mtp-select"
    :id="id"
    :name="name"
    :value="modelValue"
    :disabled="disabled"
    :aria-describedby="ariaDescribedby"
    @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
  >
    <option
      v-for="option in options"
      :key="option.value"
      :value="option.value"
      :disabled="option.disabled"
    >
      {{ option.label }}
    </option>
  </select>
</template>

<style scoped>
.mtp-select {
  min-height: 2.75rem;
  width: 100%;
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  background: var(--mtp-surface);
  color: var(--mtp-text);
  font: inherit;
  font-size: 1rem;
  padding: 0 2rem 0 0.875rem;
}

.mtp-select:focus-visible {
  border-color: var(--mtp-accent);
  outline: none;
  box-shadow: var(--mtp-focus-ring);
}
</style>
