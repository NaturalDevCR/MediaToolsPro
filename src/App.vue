<script setup lang="ts">
import { ref, onMounted, watch } from "vue";
import Header from "./components/layout/Header.vue";
import Footer from "./components/layout/Footer.vue";
import ToolsView from "./views/ToolsView.vue";
import Settings from "./views/Settings.vue";
import Logs from "./views/Logs.vue";
import { addLog } from "./stores/logs";
import { listen } from '@tauri-apps/api/event';

const currentTab = ref("tools");
const isDark = ref(false);
const unlistenLogs = ref<() => void>();

const toggleTheme = () => {
  isDark.value = !isDark.value;
  updateThemeClass();
  addLog(`Theme toggled to ${isDark.value ? 'dark' : 'light'}`, 'info', 'system');
};

const updateThemeClass = () => {
  if (isDark.value) {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
};

onMounted(async () => {
  addLog('Application started', 'success', 'system');
  
  // Check system preference
  if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
    isDark.value = true;
  }
  updateThemeClass();

  // Listen for backend logs
  unlistenLogs.value = await listen<{ message: string; level: string }>('backend-log', (event) => {
    addLog(`[BACKEND] ${event.payload.message}`, event.payload.level as any, 'system');
  });
});

watch(isDark, () => {
  updateThemeClass();
});
</script>

<template>
  <main class="h-screen w-screen overflow-hidden bg-[#f3f6fb] dark:bg-[#050a12] text-gray-900 dark:text-gray-100 flex flex-col font-sans transition-colors duration-300 relative">
    <div class="absolute inset-0 pointer-events-none bg-[radial-gradient(circle_at_top,rgba(56,189,248,0.08),transparent_32%),linear-gradient(180deg,rgba(255,255,255,0.02),transparent)] dark:bg-[radial-gradient(circle_at_top,rgba(34,211,238,0.12),transparent_28%),linear-gradient(180deg,rgba(255,255,255,0.02),transparent)]"></div>

    <Header 
      :currentTab="currentTab" 
      :isDark="isDark" 
      @update:currentTab="currentTab = $event" 
      @toggleTheme="toggleTheme"
    />

    <section class="flex-1 overflow-hidden relative z-10 flex flex-col">
      <div class="flex-1 overflow-auto scrollbar-hide">
        <Transition name="fade" mode="out-in">
          <ToolsView v-if="currentTab === 'tools'" key="tools" />
          <Logs v-else-if="currentTab === 'logs'" key="logs" />
          <Settings v-else-if="currentTab === 'settings'" key="settings" />
        </Transition>
      </div>
    </section>

    <Footer status="Ready" />
  </main>
</template>

<style>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
