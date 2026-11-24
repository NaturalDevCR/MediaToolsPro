<script setup lang="ts">
import { computed, ref, watch, nextTick, onMounted } from "vue";
import { logs, clearLogs, type LogEntry } from "../stores/logs";
import { Trash2, Terminal } from "lucide-vue-next";
import { locale, t, tMediaMessage } from "../lib/i18n";

const scopeLabels: Record<LogEntry["scope"] | "all", string> = {
  all: "Todo",
  audio: "Audio",
  video: "Vídeo",
  system: "Sistema",
};
const levelLabels: Record<LogEntry["level"], string> = {
  info: "Info",
  warn: "Aviso",
  error: "Error",
  success: "Éxito",
};
const scopes = ["all", "audio", "video", "system"] as const;

const logContainer = ref<HTMLElement | null>(null);
const scopeFilter = ref<(typeof scopes)[number]>("all");
const filteredLogs = computed(() =>
  scopeFilter.value === "all"
    ? logs.value
    : logs.value.filter((entry) => entry.scope === scopeFilter.value),
);

const scrollToBottom = async () => {
  await nextTick();
  if (logContainer.value) {
    logContainer.value.scrollTop = logContainer.value.scrollHeight;
  }
};

watch(() => filteredLogs.value.length, scrollToBottom);

onMounted(scrollToBottom);

function formatTime(date: Date) {
  return date.toLocaleTimeString(locale.value);
}

function getLevelClass(level: string) {
  switch (level) {
    case "error":
      return "text-red-500 border-red-500/20 bg-red-500/10";
    case "warn":
      return "text-yellow-500 border-yellow-500/20 bg-yellow-500/10";
    case "success":
      return "text-green-500 border-green-500/20 bg-green-500/10";
    default:
      return "text-blue-500 border-blue-500/20 bg-blue-500/10";
  }
}

function getScopeClass(scope: string) {
  switch (scope) {
    case "audio":
      return "text-cyan-300 border-cyan-400/20 bg-cyan-400/10";
    case "video":
      return "text-emerald-300 border-emerald-400/20 bg-emerald-400/10";
    case "system":
      return "text-slate-300 border-white/10 bg-white/5";
    default:
      return "text-slate-300 border-white/10 bg-white/5";
  }
}
</script>

<template>
  <div class="h-full flex flex-col">
    <!-- Toolbar -->
    <div
      class="bg-white/60 dark:bg-black/20 backdrop-blur-xl border-b border-white/20 dark:border-white/10 p-4 flex items-center justify-between shadow-sm z-20"
    >
      <div class="flex items-center gap-2 text-gray-700 dark:text-gray-200">
        <Terminal class="w-5 h-5" />
        <h2 class="font-medium">{{ t("Actividad del estudio") }}</h2>
      </div>
      <div class="flex items-center gap-3">
        <div
          class="flex rounded-xl border border-black/5 dark:border-white/10 bg-black/5 dark:bg-white/[0.04] p-1"
        >
          <button
            v-for="scope in scopes"
            :key="scope"
            :aria-pressed="scopeFilter === scope"
            @click="scopeFilter = scope"
            class="px-3 py-1.5 rounded-lg text-xs font-semibold capitalize transition-colors"
            :class="
              scopeFilter === scope
                ? 'bg-cyan-400 text-slate-950'
                : 'text-gray-600 dark:text-gray-300 hover:bg-black/5 dark:hover:bg-white/5'
            "
          >
            {{ t(scopeLabels[scope]) }}
          </button>
        </div>
        <button
          @click="clearLogs"
          class="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm text-red-500 hover:bg-red-500/10 transition-colors"
        >
          <Trash2 class="w-4 h-4" />
          {{ t("Limpiar") }}
        </button>
      </div>
    </div>

    <!-- Log List -->
    <div
      ref="logContainer"
      class="flex-1 overflow-y-auto p-4 space-y-2 font-mono text-sm"
    >
      <div
        v-if="filteredLogs.length === 0"
        class="text-center text-gray-400 py-10 italic"
      >
        {{ t("No hay actividad para mostrar") }}
      </div>

      <div
        v-for="log in filteredLogs"
        :key="log.id"
        class="flex gap-3 p-2 rounded border border-transparent hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
      >
        <span class="text-gray-400 select-none text-xs mt-0.5 w-[86px] shrink-0">{{
          formatTime(log.timestamp)
        }}</span>
        <span
          class="uppercase text-[10px] font-bold px-1.5 py-0.5 rounded border h-fit select-none"
          :class="getScopeClass(log.scope)"
        >
          {{ t(scopeLabels[log.scope]) }}
        </span>
        <span
          class="uppercase text-[10px] font-bold px-1.5 py-0.5 rounded border h-fit select-none"
          :class="getLevelClass(log.level)"
        >
          {{ t(levelLabels[log.level]) }}
        </span>
        <span class="flex-1 break-all text-gray-700 dark:text-gray-300">{{
          log.source
            ? `${log.source}: ${tMediaMessage(log.message)}`
            : tMediaMessage(log.message)
        }}</span>
      </div>
    </div>
  </div>
</template>
