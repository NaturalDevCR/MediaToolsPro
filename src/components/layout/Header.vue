<script setup lang="ts">
import {
  Sun,
  Moon,
  Settings,
  FileText,
  SlidersHorizontal,
  Languages,
} from "lucide-vue-next";
import appIcon from "../../../src-tauri/icons/mediatoolspro/128x128.png";
import { locale, setLocale, t } from "../../lib/i18n";
defineProps<{ currentTab: string; isDark: boolean }>();
defineEmits<{ "update:currentTab": [tab: string]; toggleTheme: [] }>();
const tabs = [
  { id: "tools", label: "Estudio", icon: SlidersHorizontal },
  { id: "logs", label: "Actividad", icon: FileText },
  { id: "settings", label: "Ajustes", icon: Settings },
];

function changeLanguage(event: Event) {
  const language = (event.target as HTMLSelectElement).value;
  if (language === "en" || language === "es") setLocale(language);
}
</script>
<template>
  <header class="app-header">
    <div class="brand">
      <img class="brand-symbol" :src="appIcon" alt="" width="32" height="32" />
      <span>MediaTools<span>Pro</span></span>
    </div>
    <nav class="header-nav" :aria-label="t('Navegación principal')">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        :class="{ active: currentTab === tab.id }"
        :aria-current="currentTab === tab.id ? 'page' : undefined"
        @click="$emit('update:currentTab', tab.id)"
      >
        <component :is="tab.icon" :size="14" />{{ t(tab.label) }}
      </button>
    </nav>
    <div class="header-actions">
      <span class="header-caption">{{ t("MENOS PASOS. MEJOR AUDIO.") }}</span>
      <div class="language-picker">
        <Languages :size="15" aria-hidden="true" />
        <select :aria-label="t('Idioma')" :value="locale" @change="changeLanguage">
          <option value="en" lang="en">English</option>
          <option value="es" lang="es">Español</option>
        </select>
      </div>
      <button
        class="icon-button"
        :aria-label="t(isDark ? 'Usar tema claro' : 'Usar tema oscuro')"
        @click="$emit('toggleTheme')"
      >
        <Sun v-if="isDark" :size="17" /><Moon v-else :size="17" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.language-picker {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--muted);
}
.language-picker select {
  min-height: 28px;
  padding: 3px 5px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--panel);
  color: var(--text);
  font-size: 12px;
  cursor: pointer;
}
</style>
