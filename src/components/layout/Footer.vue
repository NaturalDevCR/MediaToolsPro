<script setup lang="ts">
import { version } from "../../../package.json";
import { pendingCount } from "../../stores/studio";
</script>
<template>
  <footer class="app-footer">
    <span
      ><i class="status-dot" />{{
        pendingCount ? `${pendingCount} tarea(s) en cola` : "Estudio listo"
      }}</span
    ><span
      >TRIM / SPLIT / NORMALIZE
      <span class="muted">· v{{ version }}</span></span
    >
  </footer>
</template>
