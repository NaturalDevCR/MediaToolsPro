<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    value: number;
    max?: number;
    label?: string;
  }>(),
  {
    max: 100,
    label: "Progress",
  },
);

const percent = computed(() => {
  if (props.max <= 0) return 0;
  return Math.min(100, Math.max(0, (props.value / props.max) * 100));
});
</script>

<template>
  <div
    class="mtp-progress"
    role="progressbar"
    :aria-label="label"
    :aria-valuemin="0"
    :aria-valuemax="max"
    :aria-valuenow="value"
  >
    <div class="mtp-progress__bar" :style="{ width: `${percent}%` }"></div>
  </div>
</template>

<style scoped>
.mtp-progress {
  height: 0.5rem;
  width: 100%;
  overflow: hidden;
  border-radius: 999px;
  background: var(--mtp-surface-3);
}

.mtp-progress__bar {
  height: 100%;
  border-radius: inherit;
  background: var(--mtp-accent);
  transition: width 200ms ease;
}
</style>
