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
export type SilenceSensitivity = "conservative" | "balanced" | "sensitive";
export interface ExportEncoding {
  sampleRate?: number;
  channels?: number;
  audioBitrateKbps?: number;
  pcmBitDepth?: number;
  vorbisQuality?: number;
  flacCompression?: number;
  videoQuality?: "high" | "balanced" | "compact";
}
export interface Segment {
  startSeconds: number;
  endSeconds: number;
}
export interface ProcessJobRequest {
  inputPath: string;
  outputDir?: string;
  format: string;
  encoding?: ExportEncoding;
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
    automatic?: boolean;
    sensitivity?: SilenceSensitivity;
  };
  fadeInDuration: number;
  fadeOutDuration: number;
}
export interface AutomaticSilence {
  thresholdDb: number;
  minSilenceDuration: number;
  pauseCount: number;
  pauses: Segment[];
}
export interface SplitPlan {
  segments: Segment[];
  automatic: AutomaticSilence | null;
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
  processedPath?: string;
}

export interface SplitComparisonUpdate {
  percent: number;
  originalPath?: string;
}
