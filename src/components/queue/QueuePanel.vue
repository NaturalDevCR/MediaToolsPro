<script setup lang="ts">
import { FolderOpen } from 'lucide-vue-next';
import QueueItem from '../QueueItem.vue';
import type { QueueItemData } from '../../types/jobs';

type QueueFilter = 'studio' | 'all';

defineProps<{
  items: QueueItemData[];
  historyCount: number;
  filter: QueueFilter;
}>();

defineEmits<{
  (e: 'update:filter', value: QueueFilter): void;
  (e: 'cancel', id: string): void;
  (e: 'reveal', path: string): void;
  (e: 'process', path: string): void;
  (e: 'clearHistory'): void;
}>();
</script>

<template>
  <section class="rounded-[30px] border border-white/10 bg-[#091626] p-6">
    <div class="flex items-center justify-between gap-4 mb-5">
      <div>
        <p class="text-xs uppercase tracking-[0.22em] text-cyan-400/80 font-semibold">Queue</p>
        <h2 class="text-3xl font-semibold text-slate-50 mt-2">Downloads, processing and pipelines</h2>
      </div>
      <div class="flex items-center gap-3">
        <div class="flex rounded-2xl border border-white/10 bg-white/[0.03] p-1">
          <button
            @click="$emit('update:filter', 'studio')"
            class="px-3 py-1.5 rounded-xl text-xs font-semibold transition-colors"
            :class="filter === 'studio' ? 'bg-cyan-400 text-slate-950' : 'text-slate-300 hover:bg-white/[0.05]'"
          >
            Current Studio
          </button>
          <button
            @click="$emit('update:filter', 'all')"
            class="px-3 py-1.5 rounded-xl text-xs font-semibold transition-colors"
            :class="filter === 'all' ? 'bg-cyan-400 text-slate-950' : 'text-slate-300 hover:bg-white/[0.05]'"
          >
            All Jobs
          </button>
        </div>
        <div class="text-sm text-slate-400">
          {{ items.length }} item{{ items.length === 1 ? '' : 's' }}
        </div>
        <button
          v-if="historyCount > 0"
          @click="$emit('clearHistory')"
          class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/25 transition-colors"
        >
          Clear history
        </button>
      </div>
    </div>

    <div v-if="items.length === 0" class="py-20 flex flex-col items-center justify-center text-slate-500">
      <FolderOpen class="w-16 h-16 mb-4 opacity-50" />
      <p class="text-lg font-light text-slate-300">No jobs in this view</p>
      <p class="text-sm opacity-70">Switch the filter or add a download, pipeline, or processing job from the other tabs.</p>
    </div>

    <div v-else class="space-y-3">
      <QueueItem
        v-for="item in items"
        :key="item.id"
        :item="item"
        @cancel="$emit('cancel', $event)"
        @reveal="$emit('reveal', $event)"
        @process="$emit('process', $event)"
      />
    </div>
  </section>
</template>
