export type QueueStatus =
  | "waiting"
  | "processing"
  | "done"
  | "error"
  | "cancelled";
export type MediaKind = "audio" | "video";
export type Tool = "trim" | "split" | "normalize";
export type SplitMode = "none" | "silence" | "chapters" | "duration" | "manual";
export type SplitNormalization = "none" | "before" | "after";
export interface Segment {
  startSeconds: number;
  endSeconds: number;
}
export interface ProcessJobRequest {
  inputPath: string;
  outputDir?: string;
  format: string;
  trimStart?: string;
  trimEnd?: string;
  normalize: boolean;
  loudnessTargetLufs: number;
  truePeakDbtp: number;
  eq: { bass: number; mid: number; treble: number };
  splitMode: SplitMode;
  splitDurationSeconds?: number;
  splitPauseWindowSeconds?: number;
  outputNamePattern?: string;
  splitNormalization?: SplitNormalization;
  manualMarkers: number[];
  silence: {
    thresholdDb: number;
    minSilenceDuration: number;
    minSegmentDuration: number;
  };
  fadeInDuration: number;
  fadeOutDuration: number;
}
export interface QueueItemData {
  id: string;
  tool: Tool;
  status: QueueStatus;
  percent: number;
  title: string;
  detail?: string;
  outputPath?: string;
  error?: string;
  request: ProcessJobRequest;
}
export interface JobProgressPayload {
  id: string;
  status: QueueStatus;
  percent: number;
  detail?: string;
  outputPath?: string;
  error?: string;
}
export interface MediaProbeResponse {
  inputPath: string;
  title: string;
  durationSeconds: number;
  durationLabel: string;
  mediaKind: MediaKind;
}
export interface WaveformPreviewResponse {
  inputPath: string;
  audioPath: string;
  durationSeconds: number;
}

export interface WaveformLevelsResponse {
  intervalSeconds: number;
  peaksDb: number[];
  rmsDb: number[];
}

export interface SplitComparison {
  originalPath: string;
  processedPath: string;
}
