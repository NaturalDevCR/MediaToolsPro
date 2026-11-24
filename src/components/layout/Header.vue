<script setup lang="ts">
import {
  Sun,
  Moon,
  Settings,
  FileText,
  SlidersHorizontal,
} from "lucide-vue-next";
import appIcon from "../../../src-tauri/icons/mediatoolspro/128x128.png";
defineProps<{ currentTab: string; isDark: boolean }>();
defineEmits<{ "update:currentTab": [tab: string]; toggleTheme: [] }>();
const tabs = [
  { id: "tools", label: "Estudio", icon: SlidersHorizontal },
  { id: "logs", label: "Actividad", icon: FileText },
  { id: "settings", label: "Ajustes", icon: Settings },
];
</script>
<template>
  <header class="app-header">
    <div class="brand">
      <img class="brand-symbol" :src="appIcon" alt="" width="32" height="32" />
      <span>MediaTools<span>Pro</span></span>
    </div>
    <nav class="header-nav" aria-label="Navegación principal">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        :class="{ active: currentTab === tab.id }"
        :aria-current="currentTab === tab.id ? 'page' : undefined"
        @click="$emit('update:currentTab', tab.id)"
      >
        <component :is="tab.icon" :size="14" />{{ tab.label }}
      </button>
    </nav>
    <div class="header-actions">
      <span class="header-caption">MENOS PASOS. MEJOR AUDIO.</span
      ><button
        class="icon-button"
        :aria-label="isDark ? 'Usar tema claro' : 'Usar tema oscuro'"
        @click="$emit('toggleTheme')"
      >
        <Sun v-if="isDark" :size="17" /><Moon v-else :size="17" />
      </button>
    </div>
  </header>
</template>
