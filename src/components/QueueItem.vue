<script setup lang="ts">
import {
  FileAudio,
  FolderOpen,
  X,
  RotateCcw,
  Check,
  LoaderCircle,
} from "lucide-vue-next";
import type { QueueItemData } from "../types/jobs";
defineProps<{ item: QueueItemData }>();
defineEmits<{ cancel: []; retry: []; reveal: [path: string] }>();
const labels = {
  waiting: "En espera",
  processing: "Procesando",
  done: "Completado",
  error: "Error",
  cancelled: "Cancelado",
};
</script>
<template>
  <article class="queue-entry" :class="item.status">
    <div class="queue-icon">
      <Check v-if="item.status === 'done'" :size="18" /><LoaderCircle
        v-else-if="item.status === 'processing'"
        class="spin"
        :size="18"
      /><FileAudio v-else :size="18" />
    </div>
    <div class="queue-info">
      <div class="queue-title">
        <strong :title="item.request.inputPath">{{ item.title }}</strong
        ><span class="tag">{{ item.tool }} · {{ item.request.format }}</span>
      </div>
      <p :class="{ 'error-text': item.error }">
        {{ item.error || item.detail || labels[item.status] }}
      </p>
      <div
        v-if="item.status === 'processing'"
        class="progress-track"
        role="progressbar"
        :aria-valuenow="item.percent"
        aria-valuemin="0"
        aria-valuemax="100"
        :aria-label="item.title"
      >
        <span :style="{ width: `${item.percent}%` }" />
      </div>
    </div>
    <span class="queue-status">{{
      item.status === "processing"
        ? `${Math.round(item.percent)}%`
        : labels[item.status]
    }}</span>
    <button
      v-if="item.status === 'done' && item.outputPath"
      class="icon-button"
      title="Mostrar resultado en carpeta"
      aria-label="Mostrar resultado en carpeta"
      @click="$emit('reveal', item.outputPath)"
    >
      <FolderOpen :size="17" />
    </button>
    <button
      v-if="['error', 'cancelled'].includes(item.status)"
      class="icon-button"
      title="Reintentar"
      aria-label="Reintentar"
      @click="$emit('retry')"
    >
      <RotateCcw :size="16" />
    </button>
    <button
      class="icon-button"
      :title="
        ['waiting', 'processing'].includes(item.status)
          ? 'Cancelar tarea'
          : 'Quitar del historial'
      "
      :aria-label="
        ['waiting', 'processing'].includes(item.status)
          ? 'Cancelar tarea'
          : 'Quitar del historial'
      "
      @click="$emit('cancel')"
    >
      <X :size="17" />
    </button>
  </article>
</template>
