<script setup lang="ts">
import { computed } from "vue";
import { encodingFields } from "../lib/export-options";
import { t } from "../lib/i18n";
import type { ExportEncoding } from "../types/jobs";

const props = defineProps<{ format: string }>();
const encoding = defineModel<ExportEncoding>({ required: true });
const fields = computed(() => encodingFields(props.format));

function setOption(key: keyof ExportEncoding, event: Event) {
  const input = (event.target as HTMLSelectElement).value;
  const value = fields.value.find((field) => field.key === key)?.options
    .find((option) => String(option.value) === input)?.value;
  encoding.value = { ...encoding.value, [key]: value };
}
</script>

<template>
  <details class="export-options">
    <summary>{{ t("Opciones de codificación") }}</summary>
    <div class="export-option-fields">
      <label v-for="field in fields" :key="field.key">
        {{ t(field.label) }}
        <select :value="encoding[field.key] ?? ''" @change="setOption(field.key, $event)">
          <option value="">{{ t(field.fallback) }}</option>
          <option v-for="option in field.options" :key="option.value" :value="option.value">
            {{ t(option.label) }}
          </option>
        </select>
      </label>
    </div>
  </details>
</template>
