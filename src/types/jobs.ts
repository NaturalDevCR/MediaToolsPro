export type QueueStatus =
  | "waiting"
  | "downloading"
  | "processing"
  | "converting"
  | "done"
  | "error"
  | "cancelled";

export type JobKind = "download" | "process" | "pipeline";
export type MediaKind = "audio" | "video";
export type SplitMode = "none" | "silence" | "chapters" | "manual";
export type PlaylistMode = "auto" | "playlist" | "single";
export type AudioTarget = "general" | "azuracast";
export type VideoTarget = "general" | "google_tv_cast";

export interface EqualizerSettings {
  bass: number;
  mid: number;
  treble: number;
}

export interface SilenceSplitSettings {
  thresholdDb: number;
  minSilenceDuration: number;
  minSegmentDuration: number;
}

export interface DownloadJobRequest {
  kind: "download";
  url: string;
  format: string;
  quality: string;
  outputPath: string;
  playlistMode: PlaylistMode;
  audioTarget: AudioTarget;
  videoTarget: VideoTarget;
  cookiesFile?: string;
}

export interface ProcessJobRequest {
  kind: "process";
  inputPath: string;
  outputDir?: string;
  format: string;
  trimStart?: string;
  trimEnd?: string;
  normalize: boolean;
  loudnessTargetLufs: number;
  eq: EqualizerSettings;
  audioTarget: AudioTarget;
  splitMode: SplitMode;
  manualMarkers: number[];
  silence: SilenceSplitSettings;
  fadeInDuration: number;
  fadeOutDuration: number;
}

export interface DownloadProcessJobRequest {
  kind: "downloadProcess";
  url: string;
  downloadFormat: string;
  downloadQuality: string;
  outputDir: string;
  playlistMode: PlaylistMode;
  audioTarget: AudioTarget;
  videoTarget: VideoTarget;
  cookiesFile?: string;
  process: Omit<ProcessJobRequest, "kind" | "inputPath" | "outputDir">;
  cleanupTemp: boolean;
}

export type JobRequest = DownloadJobRequest | ProcessJobRequest | DownloadProcessJobRequest;

export interface QueueItemData {
  id: string;
  kind: JobKind;
  mediaKind: MediaKind;
  format: string;
  quality?: string;
  status: QueueStatus;
  percent: number;
  speed: string;
  eta: string;
  totalSize: string;
  title: string;
  source: string;
  detail?: string;
  outputPath?: string;
  error?: string;
  request: JobRequest;
  pipelineStage?: "download" | "process";
  tempDir?: string;
  downloadedPath?: string;
  activeBackendId?: string;
}

export interface JobProgressPayload {
  id: string;
  jobKind: JobKind;
  mediaKind: MediaKind;
  status: QueueStatus;
  percent: number;
  speed: string;
  eta: string;
  totalSize: string;
  title?: string | null;
  detail?: string | null;
  outputPath?: string | null;
  error?: string | null;
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
  imageDataUrl: string;
  audioDataUrl: string;
  durationSeconds: number;
  durationLabel: string;
  mediaKind: MediaKind;
}
