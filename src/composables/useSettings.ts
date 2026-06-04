import type {
  AudioTarget,
  EqualizerSettings,
  MediaKind,
  PlaylistMode,
  SilenceSplitSettings,
  SplitMode,
  VideoTarget,
} from "../types/jobs";
import { clamp } from "../utils/time";

export type DownloadDestinationMode = "downloads" | "custom";
export type BatchDestinationMode = "source" | "custom";

export const SETTINGS_STORAGE_KEY = "mediatoolspro.settings.v1";
export const LEGACY_SETTINGS_STORAGE_KEY = "audiotoolspro.settings.v1";

export const AUDIO_FORMATS = ["mp3", "flac", "ogg", "wav", "m4a", "aac"];
export const VIDEO_FORMATS = ["mp4", "webm", "mkv"];

export const DEFAULT_SILENCE: SilenceSplitSettings = {
  thresholdDb: -35,
  minSilenceDuration: 1.5,
  minSegmentDuration: 20,
};

export type PersistedStudioSettings = {
  format: string;
  quality: string;
  playlistMode: PlaylistMode;
  downloadAudioTarget: AudioTarget;
  downloadVideoTarget: VideoTarget;
  downloadDestinationMode: DownloadDestinationMode;
  downloadPath: string;
  batchDestinationMode: BatchDestinationMode;
  batchOutputDir: string;
  batchFormat: string;
  normalizeAudio: boolean;
  loudnessTargetLufs: number;
  batchAudioTarget: AudioTarget;
  eq: EqualizerSettings;
  fadeInDuration: number;
  fadeOutDuration: number;
  downloadPipelineSplitMode: SplitMode;
  downloadPipelineSilence: SilenceSplitSettings;
};

export function settingsKeyForMode(mode: MediaKind) {
  return `${SETTINGS_STORAGE_KEY}.${mode}`;
}

export function legacySettingsKeyForMode(mode: MediaKind) {
  return `${LEGACY_SETTINGS_STORAGE_KEY}.${mode}`;
}

export function defaultSettingsForMode(mode: MediaKind): PersistedStudioSettings {
  return {
    format: mode === "audio" ? "mp3" : "mp4",
    quality: mode === "audio" ? "best" : "1080",
    playlistMode: "auto",
    downloadAudioTarget: "general",
    downloadVideoTarget: "general",
    downloadDestinationMode: "downloads",
    downloadPath: "",
    batchDestinationMode: "source",
    batchOutputDir: "",
    batchFormat: mode === "audio" ? "mp3" : "mp4",
    normalizeAudio: mode === "audio",
    loudnessTargetLufs: -16,
    batchAudioTarget: "general",
    eq: { bass: 0, mid: 0, treble: 0 },
    fadeInDuration: 0,
    fadeOutDuration: 0,
    downloadPipelineSplitMode: "none",
    downloadPipelineSilence: { ...DEFAULT_SILENCE },
  };
}

export function sanitizeSettings(
  settings: Record<string, unknown>,
  mode: MediaKind,
): PersistedStudioSettings {
  const fallback = defaultSettingsForMode(mode);
  const formats = mode === "audio" ? AUDIO_FORMATS : VIDEO_FORMATS;
  const rawEq = settings.eq as Record<string, unknown> | undefined;
  const rawPipelineSilence = settings.downloadPipelineSilence as Record<string, unknown> | undefined;

  return {
    format:
      typeof settings.format === "string" && formats.includes(settings.format)
        ? settings.format
        : fallback.format,
    quality: typeof settings.quality === "string" ? settings.quality : fallback.quality,
    playlistMode:
      settings.playlistMode === "auto" ||
      settings.playlistMode === "playlist" ||
      settings.playlistMode === "single"
        ? settings.playlistMode
        : fallback.playlistMode,
    downloadAudioTarget:
      settings.downloadAudioTarget === "azuracast" ? "azuracast" : fallback.downloadAudioTarget,
    downloadVideoTarget:
      settings.downloadVideoTarget === "google_tv_cast"
        ? "google_tv_cast"
        : fallback.downloadVideoTarget,
    downloadDestinationMode:
      settings.downloadDestinationMode === "custom" ? "custom" : fallback.downloadDestinationMode,
    downloadPath:
      typeof settings.downloadPath === "string" ? settings.downloadPath : fallback.downloadPath,
    batchDestinationMode:
      settings.batchDestinationMode === "custom" ? "custom" : fallback.batchDestinationMode,
    batchOutputDir:
      typeof settings.batchOutputDir === "string" ? settings.batchOutputDir : fallback.batchOutputDir,
    batchFormat:
      typeof settings.batchFormat === "string" && formats.includes(settings.batchFormat)
        ? settings.batchFormat
        : fallback.batchFormat,
    normalizeAudio:
      typeof settings.normalizeAudio === "boolean" ? settings.normalizeAudio : fallback.normalizeAudio,
    loudnessTargetLufs:
      typeof settings.loudnessTargetLufs === "number"
        ? clamp(settings.loudnessTargetLufs, -30, -6)
        : fallback.loudnessTargetLufs,
    batchAudioTarget:
      settings.batchAudioTarget === "azuracast" ? "azuracast" : fallback.batchAudioTarget,
    eq: {
      bass: typeof rawEq?.bass === "number" ? rawEq.bass : fallback.eq.bass,
      mid: typeof rawEq?.mid === "number" ? rawEq.mid : fallback.eq.mid,
      treble: typeof rawEq?.treble === "number" ? rawEq.treble : fallback.eq.treble,
    },
    fadeInDuration:
      typeof settings.fadeInDuration === "number"
        ? Math.max(0, settings.fadeInDuration)
        : fallback.fadeInDuration,
    fadeOutDuration:
      typeof settings.fadeOutDuration === "number"
        ? Math.max(0, settings.fadeOutDuration)
        : fallback.fadeOutDuration,
    downloadPipelineSplitMode:
      settings.downloadPipelineSplitMode === "silence" ||
      settings.downloadPipelineSplitMode === "chapters"
        ? settings.downloadPipelineSplitMode
        : fallback.downloadPipelineSplitMode,
    downloadPipelineSilence: {
      thresholdDb:
        typeof rawPipelineSilence?.thresholdDb === "number"
          ? rawPipelineSilence.thresholdDb
          : fallback.downloadPipelineSilence.thresholdDb,
      minSilenceDuration:
        typeof rawPipelineSilence?.minSilenceDuration === "number"
          ? rawPipelineSilence.minSilenceDuration
          : fallback.downloadPipelineSilence.minSilenceDuration,
      minSegmentDuration:
        typeof rawPipelineSilence?.minSegmentDuration === "number"
          ? rawPipelineSilence.minSegmentDuration
          : fallback.downloadPipelineSilence.minSegmentDuration,
    },
  };
}

export function restorePersistedSettings(
  mode: MediaKind,
  storage: Storage = window.localStorage,
) {
  const raw =
    storage.getItem(settingsKeyForMode(mode)) ?? storage.getItem(legacySettingsKeyForMode(mode));
  if (!raw) {
    return defaultSettingsForMode(mode);
  }

  return sanitizeSettings(JSON.parse(raw) as Record<string, unknown>, mode);
}

export function savePersistedSettings(
  mode: MediaKind,
  settings: PersistedStudioSettings,
  activeMode: MediaKind,
  storage: Storage = window.localStorage,
) {
  storage.setItem(settingsKeyForMode(mode), JSON.stringify(settings));
  storage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify({ activeMode }));
}
