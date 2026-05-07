<script setup lang="ts">
import { Sun, Moon, Settings, FileText, Wrench } from 'lucide-vue-next';

defineProps<{
  currentTab: string;
  isDark: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:currentTab', tab: string): void;
  (e: 'toggleTheme'): void;
}>();

const tabs = [
  { id: 'tools', label: 'Tools', icon: Wrench },
  { id: 'logs', label: 'Logs', icon: FileText },
  { id: 'settings', label: 'Settings', icon: Settings },
];
</script>

<template>
  <header class="h-16 flex items-center justify-between px-6 select-none
    bg-white/30 dark:bg-[#030814]/80 backdrop-blur-2xl saturate-150
    border-b border-white/20 dark:border-white/10 shadow-sm transition-colors duration-300">
    
    <!-- Branding -->
    <div class="flex items-center gap-3 w-[200px]">
      <div class="w-8 h-8 rounded-xl bg-gradient-to-br from-cyan-400 to-sky-600 shadow-lg shadow-cyan-500/20"></div>
      <span class="font-bold text-lg tracking-tight text-gray-800 dark:text-gray-100">AudioToolsPro</span>
    </div>

    <!-- Navigation Tabs -->
    <nav class="flex items-center gap-1 bg-black/5 dark:bg-white/[0.04] p-1 rounded-2xl backdrop-blur-sm border border-black/5 dark:border-white/10">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        @click="emit('update:currentTab', tab.id)"
        class="flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-medium transition-all duration-200"
        :class="[
          currentTab === tab.id
            ? 'bg-white dark:bg-cyan-400/10 text-cyan-600 dark:text-cyan-300 shadow-sm'
            : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/5'
        ]"
      >
        <component :is="tab.icon" class="w-4 h-4" />
        {{ tab.label }}
      </button>
    </nav>

    <!-- Window Controls / Theme Toggle -->
    <div class="flex items-center justify-end w-[200px]">
        <button
          @click="emit('toggleTheme')"
          class="p-2 rounded-full hover:bg-black/5 dark:hover:bg-white/10 text-gray-600 dark:text-gray-400 transition-colors"
          title="Toggle Theme"
        >
          <Sun v-if="!isDark" class="w-5 h-5" />
          <Moon v-else class="w-5 h-5" />
        </button>
    </div>
  </header>
</template>
