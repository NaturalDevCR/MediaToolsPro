<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Header from "./components/layout/Header.vue";
import Footer from "./components/layout/Footer.vue";
import Studio from "./views/Studio.vue";
import Settings from "./views/Settings.vue";
import Logs from "./views/Logs.vue";
import { locale, t } from "./lib/i18n";
import { addLog, type LogEntry } from "./stores/logs";
import { disposeStudio, initStudio } from "./stores/studio";
const currentTab = ref("tools");
const savedTheme = localStorage.getItem("mediatoolspro.theme");
const isDark = ref(
  savedTheme
    ? savedTheme === "dark"
    : window.matchMedia("(prefers-color-scheme: dark)").matches,
);
let unlistenLogs: (() => void) | undefined;
let disposed = false;
watch(
  isDark,
  (value) => {
    document.documentElement.classList.toggle("dark", value);
    localStorage.setItem("mediatoolspro.theme", value ? "dark" : "light");
  },
  { immediate: true },
);
watch(locale, () => {
  document.title = `MediaToolsPro · ${t("Estudio")}`;
}, { immediate: true });
onMounted(async () => {
  addLog("Estudio iniciado", "success");
  if (!isTauri()) return;
  try {
    await initStudio();
    const unlisten = await listen<{
      message: string;
      level: LogEntry["level"];
    }>("backend-log", ({ payload }) => addLog(payload.message, payload.level));
    if (disposed) unlisten();
    else unlistenLogs = unlisten;
  } catch (error) {
    addLog(String(error), "error");
  }
});
onUnmounted(() => {
  disposed = true;
  unlistenLogs?.();
  disposeStudio();
});
</script>
<template>
  <main class="app-shell">
    <Header
      :current-tab="currentTab"
      :is-dark="isDark"
      @update:current-tab="currentTab = $event"
      @toggle-theme="isDark = !isDark"
    />
    <section class="app-content">
      <Studio
        v-show="currentTab === 'tools'"
        :active="currentTab === 'tools'"
      /><Logs v-if="currentTab === 'logs'" /><Settings
        v-if="currentTab === 'settings'"
      />
    </section>
    <Footer />
  </main>
</template>
