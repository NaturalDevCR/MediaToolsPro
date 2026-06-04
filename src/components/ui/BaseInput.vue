<script setup lang="ts">
withDefaults(
  defineProps<{
    id?: string;
    name?: string;
    modelValue: string | number;
    type?: string;
    placeholder?: string;
    autocomplete?: string;
    inputmode?: "none" | "text" | "search" | "url" | "email" | "tel" | "numeric" | "decimal";
    disabled?: boolean;
    required?: boolean;
    ariaDescribedby?: string;
  }>(),
  {
    type: "text",
    placeholder: "",
    autocomplete: undefined,
    inputmode: undefined,
    disabled: false,
    required: false,
    ariaDescribedby: undefined,
  },
);

const emit = defineEmits<{
  (event: "update:modelValue", value: string): void;
}>();
</script>

<template>
  <input
    class="mtp-input"
    :id="id"
    :name="name"
    :type="type"
    :value="modelValue"
    :placeholder="placeholder"
    :autocomplete="autocomplete"
    :inputmode="inputmode"
    :disabled="disabled"
    :required="required"
    :aria-describedby="ariaDescribedby"
    @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
  />
</template>

<style scoped>
.mtp-input {
  min-height: 2.75rem;
  width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  background: var(--mtp-surface);
  color: var(--mtp-text);
  font: inherit;
  font-size: 1rem;
  padding: 0 0.875rem;
}

.mtp-input::placeholder {
  color: var(--mtp-text-subtle);
}

.mtp-input:focus-visible {
  border-color: var(--mtp-accent);
  outline: none;
  box-shadow: var(--mtp-focus-ring);
}
</style>
