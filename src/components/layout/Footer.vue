<script setup lang="ts">
import { version } from "../../../package.json";
import { t } from "../../lib/i18n";
import { pendingCount } from "../../stores/studio";
</script>
<template>
  <footer class="app-footer">
    <span
      ><i class="status-dot" />{{
        pendingCount === 0
          ? t("Estudio listo")
          : pendingCount === 1
            ? t("1 tarea en cola")
            : t("{count} tareas en cola", { count: pendingCount })
      }}</span
    ><span
      >TRIM / SPLIT / NORMALIZE
      <span class="muted">· v{{ version }}</span></span
    >
  </footer>
</template>
