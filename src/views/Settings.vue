<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  Activity,
  CheckCircle2,
  CircleAlert,
  RotateCcw,
  LoaderCircle,
} from "lucide-vue-next";
import { t } from "../lib/i18n";

const status = ref<{ ffmpeg: boolean; ffmpeg_version?: string }>({
  ffmpeg: false,
});
const checking = ref(false);
const installing = ref(false);
const error = ref("");
async function check() {
  if (!isTauri()) return;
  checking.value = true;
  error.value = "";
  try {
    status.value = await invoke("check_binaries");
  } catch (e) {
    error.value = String(e);
  } finally {
    checking.value = false;
  }
}
async function install() {
  installing.value = true;
  error.value = "";
  try {
    await invoke("install_ffmpeg");
    await check();
  } catch (e) {
    error.value = String(e);
  } finally {
    installing.value = false;
  }
}
onMounted(check);
</script>
<template>
  <div class="settings-page">
    <div class="eyebrow">{{ t("CONFIGURACIÓN DEL ESTUDIO") }}</div>
    <h1>{{ t("Ajustes") }}<span>.</span></h1>
    <div class="panel">
      <div class="section-top">
        <div>
          <Activity :size="17" />
          <h2>{{ t("Motor de procesamiento") }}</h2>
        </div>
        <span class="tag">FFMPEG</span>
      </div>
      <div class="engine-body">
        <div
          class="engine-status"
          :class="{ missing: !status.ffmpeg && !checking }"
        >
          <LoaderCircle v-if="checking" :size="18" class="spin" /><CheckCircle2
            v-else-if="status.ffmpeg"
            :size="18"
          /><CircleAlert v-else :size="18" />{{
            t(checking
              ? "Comprobando motor…"
              : status.ffmpeg
                ? "Motor disponible"
                : "Motor no disponible")
          }}
        </div>
        <p>
          {{ t("Trim, Split y Normalize utilizan FFmpeg en tu equipo. El estudio reconoce tanto la instalación de MediaToolsPro como una instalación del sistema. El procesamiento de tus archivos se realiza localmente.") }}
        </p>
        <div v-if="status.ffmpeg_version" class="engine-version">
          {{ status.ffmpeg_version }}
        </div>
        <div class="engine-actions">
          <button
            class="secondary-button"
            :disabled="checking || installing || !isTauri()"
            @click="check"
          >
            <RotateCcw :size="15" />{{ t("Volver a comprobar") }}</button
          ><button
            v-if="!status.ffmpeg"
            class="primary-button"
            :disabled="installing || checking || !isTauri()"
            @click="install"
          >
            <LoaderCircle v-if="installing" :size="15" class="spin" />{{
              t(installing ? "Instalando motor…" : "Instalar FFmpeg")
            }}
          </button>
        </div>
        <p v-if="installing">
          {{ t("La instalación necesita conexión a internet y puede tardar unos minutos.") }}
        </p>
        <p v-if="error" class="error-text" role="alert">{{ t(error) }}</p>
      </div>
    </div>
  </div>
</template>
