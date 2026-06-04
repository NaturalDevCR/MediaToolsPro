<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  id: string;
  label: string;
  hint?: string;
  error?: string;
  required?: boolean;
}>();

const describedBy = computed(() => {
  const ids = [];
  if (props.hint) ids.push(`${props.id}-hint`);
  if (props.error) ids.push(`${props.id}-error`);
  return ids.join(" ") || undefined;
});
</script>

<template>
  <div class="mtp-field">
    <label class="mtp-field__label" :for="id">
      {{ label }}
      <span v-if="required" aria-hidden="true">*</span>
    </label>
    <slot :id="id" :described-by="describedBy" />
    <p v-if="hint" :id="`${id}-hint`" class="mtp-field__hint">
      {{ hint }}
    </p>
    <p v-if="error" :id="`${id}-error`" class="mtp-field__error" aria-live="polite">
      {{ error }}
    </p>
  </div>
</template>

<style scoped>
.mtp-field {
  display: grid;
  gap: 0.375rem;
}

.mtp-field__label {
  color: var(--mtp-text);
  font-size: 0.875rem;
  font-weight: 650;
}

.mtp-field__hint,
.mtp-field__error {
  margin: 0;
  font-size: 0.8125rem;
  line-height: 1.4;
}

.mtp-field__hint {
  color: var(--mtp-text-muted);
}

.mtp-field__error {
  color: var(--mtp-danger);
}
</style>
