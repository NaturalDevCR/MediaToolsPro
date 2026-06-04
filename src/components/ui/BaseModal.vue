<script setup lang="ts">
withDefaults(
  defineProps<{
    open: boolean;
    title: string;
  }>(),
  {
    open: false,
  },
);

const emit = defineEmits<{
  (event: "close"): void;
}>();
</script>

<template>
  <dialog v-if="open" class="mtp-modal" open :aria-label="title" @cancel.prevent="emit('close')">
    <header class="mtp-modal__header">
      <h2>{{ title }}</h2>
      <button type="button" class="mtp-modal__close" aria-label="Close" @click="emit('close')">
        x
      </button>
    </header>
    <div class="mtp-modal__body">
      <slot />
    </div>
  </dialog>
</template>

<style scoped>
.mtp-modal {
  width: min(36rem, calc(100vw - 2rem));
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-md);
  background: var(--mtp-surface);
  color: var(--mtp-text);
  box-shadow: var(--mtp-shadow-md);
  padding: 0;
}

.mtp-modal::backdrop {
  background: rgb(2 6 23 / 0.52);
}

.mtp-modal__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  border-bottom: 1px solid var(--mtp-border);
  padding: 1rem;
}

.mtp-modal__header h2 {
  margin: 0;
  font-size: 1rem;
  font-weight: 700;
}

.mtp-modal__body {
  padding: 1rem;
}

.mtp-modal__close {
  width: 2rem;
  height: 2rem;
  border: 1px solid var(--mtp-border);
  border-radius: var(--mtp-radius-sm);
  background: var(--mtp-surface-2);
  color: var(--mtp-text);
}
</style>
