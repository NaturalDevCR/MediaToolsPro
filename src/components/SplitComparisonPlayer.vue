<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Pause, Play, Square } from "lucide-vue-next";
import type { SplitComparison } from "../types/jobs";
import { t } from "../lib/i18n";
import { formatPlaybackTime as time } from "../lib/playback";

const props = defineProps<{ comparison: SplitComparison; duration: number }>();
const emit = defineEmits<{ play: []; error: [message: string] }>();
type Version = "original" | "processed";
const original = ref<HTMLAudioElement>();
const processed = ref<HTMLAudioElement>();
const active = ref<Version>("original");
const position = ref(0);
const playing = ref(false);
const starting = ref(false);
const playbackActive = computed(() => playing.value || starting.value);
const sharedSource = computed(() => props.comparison.originalPath === props.comparison.processedPath);
let playRequest = 0;

function player(version = active.value) {
  return version === "original" || sharedSource.value ? original.value : processed.value;
}

function pause() {
  playRequest++;
  playing.value = false;
  starting.value = false;
  original.value?.pause();
  processed.value?.pause();
}

function stop() {
  pause();
  seekTo(0);
}

async function play() {
  const target = player();
  if (!target) return;
  if (target.ended || position.value >= props.duration) seekTo(0);
  const request = ++playRequest;
  starting.value = true;
  emit("play");
  try {
    await target.play();
  } catch {
    if (request === playRequest) emit("error", "No se pudo reproducir la preescucha A/B.");
  } finally {
    if (request === playRequest) starting.value = false;
  }
}

function selectVersion(version: Version) {
  const previous = player();
  const target = player(version);
  if (!target || active.value === version) return;
  if (previous === target) {
    active.value = version;
    return;
  }
  const elapsed = previous?.currentTime ?? position.value;
  const resume = playbackActive.value;
  pause();
  active.value = version;
  seekTo(elapsed);
  if (resume) void play();
}

function sync(version: Version) {
  const audio = player(version);
  if (!audio || player() !== audio) return;
  playing.value = !audio.paused && !audio.ended;
  position.value = Math.min(audio.currentTime, props.duration);
}

function seekTo(seconds: number) {
  const audio = player();
  if (!audio) return;
  const end = Number.isFinite(audio.duration) ? Math.min(audio.duration, props.duration) : props.duration;
  audio.currentTime = Math.max(0, Math.min(seconds, end));
  position.value = audio.currentTime;
}

function seek(event: Event) {
  seekTo(Number((event.target as HTMLInputElement).value));
}

function loadError(version: Version, message: string) {
  if (player() === player(version)) pause();
  emit("error", message);
}

defineExpose({ pause, stop });
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
      <div class="split-mode-tabs" :aria-label="t('Versión del audio')">
        <button :class="{ active: active === 'original' }" :aria-pressed="active === 'original'"
          @click="selectVersion('original')">A · Original</button>
        <button :class="{ active: active === 'processed' }" :aria-pressed="active === 'processed'"
          :disabled="!comparison.processedPath" @click="selectVersion('processed')">{{ t(sharedSource ? 'B · Sin cambios' : 'B · Con fades') }}</button>
      </div>
    </div>
    <div class="comparison-transport">
      <button class="secondary-button compact" :aria-label="t(playbackActive ? 'Pausar comparación' : 'Reproducir comparación')"
        @click="playbackActive ? pause() : play()">
        <Pause v-if="playbackActive" :size="15" /><Play v-else :size="15" />
        {{ t(playbackActive ? 'Pausar' : 'Reproducir') }}
      </button>
      <button class="secondary-button compact" :aria-label="t('Detener comparación')" @click="stop">
        <Square :size="13" /> {{ t('Detener') }}
      </button>
      <span class="timecode">{{ time(position) }} <span class="time-total">/ {{ time(duration) }}</span></span>
    </div>
    <input type="range" min="0" :max="duration" step="0.001" :value="position" class="comparison-progress"
      :aria-label="t('Posición de la comparación')" :aria-valuetext="`${time(position)} / ${time(duration)}`" @input="seek" />
    <audio ref="original" preload="metadata" :src="convertFileSrc(comparison.originalPath)"
      @playing="sync('original')" @pause="sync('original')" @timeupdate="sync('original')" @ended="sync('original')"
      @error="loadError('original', 'No se pudo cargar la versión original.')" />
    <audio v-if="comparison.processedPath && !sharedSource" ref="processed" preload="metadata" :src="convertFileSrc(comparison.processedPath)"
      @playing="sync('processed')" @pause="sync('processed')" @timeupdate="sync('processed')" @ended="sync('processed')"
      @error="loadError('processed', 'No se pudo cargar la versión procesada.')" />
  </div>
</template>
