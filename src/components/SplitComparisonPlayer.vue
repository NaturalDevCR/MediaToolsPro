<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Pause, Play } from "lucide-vue-next";
import type { SplitComparison } from "../types/jobs";
import { t } from "../lib/i18n";

const props = defineProps<{ comparison: SplitComparison; duration: number }>();
const emit = defineEmits<{ play: []; error: [message: string] }>();
type Version = "original" | "processed";
const original = ref<HTMLAudioElement>();
const processed = ref<HTMLAudioElement>();
const active = ref<Version>("original");
const position = ref(0);
const playing = ref(false);

function player(version = active.value) {
  return version === "original" ? original.value : processed.value;
}

function pause() {
  original.value?.pause();
  processed.value?.pause();
}

async function play(version = active.value) {
  const previous = player();
  const target = player(version);
  if (!target) return;
  const at = previous?.ended ? 0 : previous?.currentTime ?? 0;
  if (previous !== target) previous?.pause();
  active.value = version;
  target.currentTime = Math.min(at, Number.isFinite(target.duration) ? target.duration : props.duration);
  emit("play");
  try {
    await target.play();
  } catch (error) {
    if (active.value === version && !(error instanceof DOMException && error.name === "AbortError")) {
      emit("error", "No se pudo reproducir la preescucha A/B.");
    }
  }
}

function sync(version: Version) {
  const audio = player(version);
  if (active.value !== version || !audio) return;
  playing.value = !audio.paused && !audio.ended;
  position.value = audio.currentTime;
}

function seek(event: Event) {
  const audio = player();
  if (!audio) return;
  audio.currentTime = Number((event.target as HTMLInputElement).value);
  position.value = audio.currentTime;
}

defineExpose({ pause });
onBeforeUnmount(() => {
  pause();
  for (const audio of [original.value, processed.value]) {
    audio?.removeAttribute("src");
    audio?.load();
  }
});
</script>

<template>
  <div class="comparison-player">
    <div class="comparison-actions">
      <button class="icon-button" :aria-label="t(playing ? 'Pausar comparación' : 'Reproducir comparación')"
        @click="playing ? pause() : play()">
        <Pause v-if="playing" :size="15" /><Play v-else :size="15" />
      </button>
      <div class="split-mode-tabs" :aria-label="t('Versión del audio')">
        <button :class="{ active: active === 'original' }" :aria-pressed="active === 'original'"
          @click="play('original')">A · Original</button>
        <button :class="{ active: active === 'processed' }" :aria-pressed="active === 'processed'"
          @click="play('processed')">{{ t("B · Procesado") }}</button>
      </div>
    </div>
    <input type="range" min="0" :max="duration" step="0.001" :value="position"
      :aria-label="t('Posición de la comparación')" @input="seek" />
    <audio ref="original" preload="metadata" :src="convertFileSrc(comparison.originalPath)"
      @play="sync('original')" @pause="sync('original')" @timeupdate="sync('original')" @ended="sync('original')"
      @error="emit('error', 'No se pudo cargar la versión original.')" />
    <audio ref="processed" preload="metadata" :src="convertFileSrc(comparison.processedPath)"
      @play="sync('processed')" @pause="sync('processed')" @timeupdate="sync('processed')" @ended="sync('processed')"
      @error="emit('error', 'No se pudo cargar la versión procesada.')" />
  </div>
</template>
