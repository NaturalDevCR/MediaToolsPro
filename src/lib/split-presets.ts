import type { ExportEncoding, SilenceSensitivity, SplitMode, SplitNormalization } from "../types/jobs";

export const splitPresetsKey = "mediatoolspro.split.presets.v1";
export const audioFormats = ["wav", "flac", "aiff", "mp3", "m4a", "aac", "ogg", "opus"];
export const videoFormats = ["mp4", "mkv", "mov", "webm", ...audioFormats];

export interface SplitPreset {
  id: string;
  name: string;
  mode: Exclude<SplitMode, "none">;
  duration: number;
  snapToPause: boolean;
  pauseWindow: number;
  snapMinSilence: number;
  autoSilence?: boolean;
  sensitivity?: SilenceSensitivity;
  scanAutomatically?: boolean;
  thresholdDb: number;
  minSilence: number;
  minSegment: number;
  fadeIn: number;
  fadeOut: number;
  normalization: SplitNormalization;
  loudness: number;
  truePeak: number;
  format: string;
  encoding?: ExportEncoding;
  outputNamePattern: string;
}

export function validOutputNamePattern(value: string) {
  const pattern = value.trim();
  if (!pattern) return true;
  const literals = pattern.replace(/\{(?:archivo|numero)\}/g, "");
  return new TextEncoder().encode(pattern).length <= 100 &&
    pattern.includes("{numero}") &&
    !/[\\/:*?"<>|{}\u0000-\u001f\u007f-\u009f]/u.test(literals);
}

export function isSplitPreset(value: unknown): value is SplitPreset {
  if (!value || typeof value !== "object") return false;
  const preset = value as Partial<SplitPreset>;
  const range = (number: unknown, min: number, max = Infinity) =>
    typeof number === "number" && Number.isFinite(number) && number >= min && number <= max;
  return typeof preset.id === "string" && Boolean(preset.id) &&
    typeof preset.name === "string" && Boolean(preset.name.trim()) &&
    ["chapters", "silence", "duration", "manual"].includes(preset.mode ?? "") &&
    range(preset.duration, 0.001) && typeof preset.snapToPause === "boolean" &&
    range(preset.pauseWindow, 0.001, 60) && range(preset.snapMinSilence, 0.05, 60) &&
    (preset.autoSilence === undefined || typeof preset.autoSilence === "boolean") &&
    (preset.sensitivity === undefined || ["conservative", "balanced", "sensitive"].includes(preset.sensitivity)) &&
    (preset.scanAutomatically === undefined || typeof preset.scanAutomatically === "boolean") &&
    range(preset.thresholdDb, -80, -5) &&
    range(preset.minSilence, 0.05, 60) && range(preset.minSegment, 0.001) &&
    range(preset.fadeIn, 0) && range(preset.fadeOut, 0) &&
    ["none", "before", "after"].includes(preset.normalization ?? "") &&
    range(preset.loudness, -30, -6) && range(preset.truePeak, -9, 0) &&
    videoFormats.includes(preset.format ?? "") &&
    (preset.encoding === undefined || Boolean(preset.encoding) && typeof preset.encoding === "object") &&
    typeof preset.outputNamePattern === "string" &&
    validOutputNamePattern(preset.outputNamePattern);
}

export function parseSplitPresets(raw: string | null): SplitPreset[] {
  try {
    const value: unknown = JSON.parse(raw ?? "[]");
    return Array.isArray(value) ? value.filter(isSplitPreset) : [];
  } catch {
    return [];
  }
}
