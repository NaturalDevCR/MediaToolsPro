<script setup lang="ts">
import { computed } from 'vue';
import { FileAudio, FileVideo, FolderOpen, X, Wrench } from 'lucide-vue-next';
import type { QueueItemData } from '../types/jobs';

const props = defineProps<{
  item: QueueItemData;
}>();

const emit = defineEmits<{
  (e: 'cancel', id: string): void;
  (e: 'reveal', path: string): void;
  (e: 'process', path: string): void;
}>();

const statusColor = computed(() => {
  switch (props.item.status) {
    case 'waiting':
      return 'bg-slate-500/60';
    case 'downloading':
      return 'bg-cyan-400';
    case 'processing':
      return 'bg-cyan-300';
    case 'converting':
      return 'bg-sky-400';
    case 'done':
      return 'bg-emerald-400';
    case 'error':
      return 'bg-rose-400';
    case 'cancelled':
      return 'bg-slate-400';
    default:
      return 'bg-slate-500/60';
  }
});

const statusLabel = computed(() => {
  switch (props.item.status) {
    case 'downloading':
      return 'Downloading';
    case 'processing':
      return 'Processing';
    case 'converting':
      return 'Converting';
    case 'done':
      return 'Done';
    case 'error':
      return 'Error';
    case 'cancelled':
      return 'Cancelled';
    default:
      return 'Waiting';
  }
});

const kindLabel = computed(() => {
  if (props.item.kind === 'pipeline') {
    return props.item.pipelineStage === 'process' ? 'pipeline: process' : 'pipeline: download';
  }

  return props.item.kind;
});

const actionLabel = computed(() => {
  return ['waiting', 'downloading', 'processing', 'converting'].includes(props.item.status)
    ? 'Cancel'
    : 'Remove';
});

const canRevealOutput = computed(() => props.item.status === 'done' && Boolean(props.item.outputPath));
</script>

<template>
  <div class="group relative rounded-[24px] border border-black/5 dark:border-white/10 bg-white/70 dark:bg-[#07111d] p-4 shadow-sm transition-all hover:border-cyan-400/20">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl flex items-center justify-center bg-slate-100 dark:bg-white/5 text-slate-500 dark:text-slate-300 border border-black/5 dark:border-white/10">
        <FileAudio v-if="item.mediaKind === 'audio'" class="w-5 h-5" />
        <FileVideo v-else class="w-5 h-5" />
      </div>

      <div class="flex-1 min-w-0">
        <div class="flex justify-between items-start">
          <div class="min-w-0 pr-2">
            <h3 class="text-sm font-medium text-slate-800 dark:text-slate-100 truncate" :title="item.title || item.source">
              {{ item.title || item.source }}
            </h3>
            <p class="text-xs text-slate-500 dark:text-slate-400 truncate mt-1" :title="item.outputPath || item.detail || item.source">
              {{ item.outputPath || item.detail || item.source }}
            </p>
          </div>

          <div class="flex items-center gap-1">
            <button
              v-if="canRevealOutput && item.outputPath"
              @click="emit('reveal', item.outputPath)"
              class="text-slate-400 hover:text-cyan-300 transition-colors p-1 rounded-md hover:bg-cyan-50 dark:hover:bg-cyan-900/20"
              title="Show in folder"
            >
              <FolderOpen class="w-4 h-4" />
            </button>

            <button
              v-if="item.status === 'done' && item.outputPath"
              @click="emit('process', item.outputPath)"
              class="text-slate-400 hover:text-emerald-300 transition-colors p-1 rounded-md hover:bg-emerald-50 dark:hover:bg-emerald-900/20"
              title="Send to Process"
            >
              <Wrench class="w-4 h-4" />
            </button>

            <button
              @click="emit('cancel', item.id)"
              class="text-slate-400 hover:text-rose-500 transition-colors p-1 rounded-md hover:bg-rose-50 dark:hover:bg-rose-900/20"
              :title="actionLabel"
            >
              <X class="w-4 h-4" />
            </button>
          </div>
        </div>

        <div class="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 mt-1 gap-4">
          <div class="flex items-center gap-2 flex-wrap">
            <span class="px-2 py-0.5 rounded-full bg-slate-900/5 dark:bg-white/10 uppercase tracking-wide text-[10px] font-semibold">
              {{ kindLabel }}
            </span>
            <span class="capitalize font-medium">
              {{ item.format.toUpperCase() }} {{ item.quality && item.quality !== 'best' ? `(${item.quality})` : '' }}
            </span>
            <span class="w-1 h-1 bg-slate-300 dark:bg-slate-600 rounded-full"></span>
            <span>{{ statusLabel }}</span>
          </div>

          <span v-if="['downloading', 'processing', 'converting'].includes(item.status)">
            {{ item.speed !== '-' ? item.speed : 'Running' }}
            <template v-if="item.eta && item.eta !== '-'"> • ETA {{ item.eta }}</template>
          </span>
          <span v-else-if="item.status === 'done'">Ready</span>
          <span v-else-if="item.status === 'error'" class="text-rose-500">Needs attention</span>
        </div>

        <p v-if="item.error" class="text-xs text-rose-500 dark:text-rose-400 mt-2 truncate" :title="item.error">
          {{ item.error }}
        </p>

        <div class="h-1.5 w-full bg-slate-200 dark:bg-white/10 rounded-full mt-2 overflow-hidden relative">
          <div
            class="h-full transition-all duration-300 rounded-full relative overflow-hidden"
            :class="statusColor"
            :style="{ width: `${item.percent}%` }"
          >
            <div
              v-if="['downloading', 'processing', 'converting'].includes(item.status)"
              class="absolute inset-0 bg-white/20 animate-[shimmer_2s_infinite]"
            ></div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@keyframes shimmer {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(100%); }
}
</style>
