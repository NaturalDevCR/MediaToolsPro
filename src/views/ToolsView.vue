<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { downloadDir } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';
import {
  FolderOpen,
  Pause,
  Play,
  Repeat2,
  RotateCcw,
  RotateCw,
  Scissors,
  Settings2,
  SlidersHorizontal,
  X,
  Youtube,
} from 'lucide-vue-next';
import QueueItem from '../components/QueueItem.vue';
import {
  AUDIO_FORMATS,
  DEFAULT_SILENCE,
  LEGACY_SETTINGS_STORAGE_KEY,
  SETTINGS_STORAGE_KEY,
  VIDEO_FORMATS,
  restorePersistedSettings,
  sanitizeSettings,
  savePersistedSettings,
  type BatchDestinationMode,
  type DownloadDestinationMode,
  type PersistedStudioSettings,
} from '../composables/useSettings';
import {
  applyJobProgress,
  getQueueStats,
  isActiveStatus,
  isTerminalStatus,
} from '../composables/useQueue';
import { addLog as pushLog } from '../stores/logs';
import type {
  AudioTarget,
  DownloadJobRequest,
  EqualizerSettings,
  JobRequest,
  JobProgressPayload,
  MediaKind,
  MediaProbeResponse,
  PlaylistMode,
  ProcessJobRequest,
  QueueItemData,
  QueueStatus,
  SilenceSplitSettings,
  SplitMode,
  VideoTarget,
  WaveformPreviewResponse,
  YtdlpFormatItem,
  YtdlpFormatsResponse,
} from '../types/jobs';

type WorkspaceTab = 'downloads' | 'process' | 'split' | 'queue';
type QueueFilter = 'studio' | 'all';
type TrimHandle = 'start' | 'end';
type SummaryRow = { label: string; value: string; detail?: string };
type OverlayPanel =
  | 'downloadOptions'
  | 'downloadAccess'
  | 'processOutput'
  | 'processAudio'
  | 'trimTools';

const DOWNLOAD_CONCURRENT_LIMIT = 2;
const PROCESS_CONCURRENT_LIMIT = 2;
const QUEUE_STORAGE_KEY = 'mediatoolspro.queue.v1';
const LEGACY_QUEUE_STORAGE_KEY = 'audiotoolspro.queue.v1';
const AUDIO_BITRATES = ['320', '256', '192', '128'];
const VIDEO_QUALITIES = ['2160', '1080', '720', '480'];
const MIN_TRIM_GAP_SECONDS = 0.1;
const TABS: Array<{ value: WorkspaceTab; label: string; hint: string }> = [
  { value: 'downloads', label: 'Downloads', hint: 'URL, playlist, cookies' },
  { value: 'process', label: 'Process', hint: 'Export, normalize, EQ' },
  { value: 'split', label: 'Split / Trim', hint: 'Waveform, marks, silence' },
  { value: 'queue', label: 'Queue', hint: 'Track every job' },
];

const AUDIO_TARGETS: Array<{ value: AudioTarget; label: string; hint: string }> = [
  { value: 'general', label: 'General audio', hint: 'Normal download and export behavior.' },
  {
    value: 'azuracast',
    label: 'AzuraCast Ready',
    hint: 'MP3, 320 kbps, 44.1 kHz stereo and offline loudness prep.',
  },
];

const LOUDNESS_PRESETS = [
  { value: -14, label: 'Streaming loud' },
  { value: -16, label: 'Podcast / web' },
  { value: -18, label: 'Broadcast light' },
  { value: -23, label: 'EBU R128' },
];

const PLAYLIST_MODES: Array<{ value: PlaylistMode; label: string; hint: string }> = [
  { value: 'auto', label: 'Auto detect', hint: 'Use the full playlist when the URL includes one.' },
  { value: 'playlist', label: 'Full playlist', hint: 'Force every item in the playlist to download.' },
  { value: 'single', label: 'Single item', hint: 'Ignore playlist context and fetch only one item.' },
];

const SPLIT_MODES: Array<{ value: SplitMode; label: string; hint: string }> = [
  { value: 'none', label: 'Trim only', hint: 'Keep one output and only trim it.' },
  { value: 'silence', label: 'Auto split', hint: 'Cut long mixes by silence detection.' },
  {
    value: 'chapters',
    label: 'Source chapters',
    hint: 'Use embedded YouTube chapters, falling back to silence.',
  },
  { value: 'manual', label: 'Manual marks', hint: 'Use your own cut points on the timeline.' },
];

type StudioMode = MediaKind;

const activeTab = ref<WorkspaceTab>('downloads');
const studioMode = ref<StudioMode>('audio');
const systemDownloadDir = ref('Downloads');

const url = ref('');
const format = ref('mp3');
const quality = ref('best');
const playlistMode = ref<PlaylistMode>('auto');
const downloadAudioTarget = ref<AudioTarget>('general');
const downloadVideoTarget = ref<VideoTarget>('general');
const downloadDestinationMode = ref<DownloadDestinationMode>('downloads');
const downloadPath = ref('');
const cookiesFile = ref('');

const batchFiles = ref<string[]>([]);
const batchDestinationMode = ref<BatchDestinationMode>('source');
const batchOutputDir = ref('');
const batchFormat = ref('mp3');
const trimStart = ref('');
const trimEnd = ref('');
const normalizeAudio = ref(true);
const loudnessTargetLufs = ref(-16);
const batchAudioTarget = ref<AudioTarget>('general');
const eq = ref<EqualizerSettings>({
  bass: 0,
  mid: 0,
  treble: 0,
});

const splitMode = ref<SplitMode>('none');
const silence = ref<SilenceSplitSettings>({ ...DEFAULT_SILENCE });
const downloadPipelineSplitMode = ref<SplitMode>('none');
const downloadPipelineSilence = ref<SilenceSplitSettings>({ ...DEFAULT_SILENCE });
const fadeInDuration = ref(0);
const fadeOutDuration = ref(0);
const mediaProbe = ref<MediaProbeResponse | null>(null);
const waveformPreview = ref<WaveformPreviewResponse | null>(null);
const waveformLoading = ref(false);
const waveformError = ref('');
const timelineCursor = ref(0);
const manualMarkers = ref<number[]>([]);
const manualMarkerInput = ref('');
const waveformTrack = ref<HTMLElement | null>(null);
const audioPreviewElement = ref<HTMLAudioElement | null>(null);
const activeTrimHandle = ref<TrimHandle | null>(null);
const isPreviewPlaying = ref(false);
const loopSelection = ref(false);
const activeOverlay = ref<OverlayPanel | null>(null);
const queueFilter = ref<QueueFilter>('studio');

const queue = ref<QueueItemData[]>([]);
const unlisteners = ref<(() => void)[]>([]);

// Format explorer state
const downloadStep = ref<'url' | 'formats' | 'confirm'>('url');
const formatList = ref<YtdlpFormatItem[]>([]);
const selectedFormatId = ref<string | null>(null);
const formatListLoading = ref(false);
const formatListError = ref('');
const formatListTitle = ref('');

let waveformRequestToken = 0;
let stopPointerTracking: (() => void) | null = null;
let queuePersistenceTimer: number | null = null;

const isAudioStudio = computed(() => studioMode.value === 'audio');
const activeDownloadFormats = computed(() => (isAudioStudio.value ? AUDIO_FORMATS : VIDEO_FORMATS));
const activeBatchFormats = computed(() => (isAudioStudio.value ? AUDIO_FORMATS : VIDEO_FORMATS));
const isAudioDownload = computed(() => isAudioStudio.value && AUDIO_FORMATS.includes(format.value));
const isAudioBatch = computed(() => isAudioStudio.value && AUDIO_FORMATS.includes(batchFormat.value));
const isAutoSplitMode = computed(() => splitMode.value === 'silence');
const isChapterSplitMode = computed(() => splitMode.value === 'chapters');
const isManualSplitMode = computed(() => splitMode.value === 'manual');
const hasBatchFiles = computed(() => batchFiles.value.length > 0);
const hasSingleBatchFile = computed(() => batchFiles.value.length === 1);
const selectedBatchFile = computed(() => (hasSingleBatchFile.value ? batchFiles.value[0] : ''));
const currentDuration = computed(
  () => mediaProbe.value?.durationSeconds ?? waveformPreview.value?.durationSeconds ?? 0,
);
const waveformImageUrl = computed(() => waveformPreview.value?.imageDataUrl ?? '');
const audioPreviewUrl = computed(() => waveformPreview.value?.audioDataUrl ?? '');
const resolvedDownloadPath = computed(() =>
  downloadDestinationMode.value === 'custom' ? downloadPath.value : systemDownloadDir.value,
);
const downloadFormatSummary = computed(() => {
  if (selectedFormat.value) {
    const f = selectedFormat.value;
    return `${f.resolution || f.formatNote || f.formatId} • ${f.ext.toUpperCase()}`;
  }
  return isAudioDownload.value
    ? `${format.value.toUpperCase()} • ${quality.value === 'best' ? 'Best available' : `${quality.value} kbps`}`
    : `${format.value.toUpperCase()} • ${quality.value === 'best' ? 'Best available' : `${quality.value}p`}`;
});
const downloadAccessSummary = computed(() => {
  if (cookiesFile.value.trim()) {
    return 'Saved internally';
  }

  return 'Not enabled';
});
const savedCookiesStateLabel = computed(() =>
  cookiesFile.value.trim() ? 'Internal copy ready' : 'No cookies file saved',
);
const savedCookiesActionLabel = computed(() =>
  cookiesFile.value.trim() ? 'Replace cookies.txt' : 'Import cookies.txt',
);
const savedCookiesHelpText = computed(() =>
  cookiesFile.value.trim()
    ? 'MediaToolsPro will use the saved internal copy even if the original export gets moved or deleted.'
    : 'Import a cookies.txt export only when a source needs an authenticated session.',
);
const currentPlaylistModeHint = computed(() => {
  return PLAYLIST_MODES.find((mode) => mode.value === playlistMode.value)?.hint ?? '';
});
const timelineCursorLabel = computed(() => formatDisplayTime(timelineCursor.value));
const trimSelection = computed(() =>
  normalizeTrimBounds(
    currentDuration.value,
    parseTimeInput(trimStart.value) ?? 0,
    parseTimeInput(trimEnd.value) ?? currentDuration.value,
  ),
);
const trimStartPercent = computed(() => markerPosition(trimSelection.value.start));
const trimEndPercent = computed(() => markerPosition(trimSelection.value.end));
const timelineCursorPercent = computed(() => markerPosition(timelineCursor.value));
const trimSelectionWidth = computed(() =>
  Math.max(0, trimEndPercent.value - trimStartPercent.value),
);
const manualSplitBlocked = computed(
  () => splitMode.value === 'manual' && batchFiles.value.length !== 1,
);
const downloadReady = computed(() => {
  if (!url.value.trim()) {
    return false;
  }

  if (downloadDestinationMode.value === 'custom' && !downloadPath.value) {
    return false;
  }

  return true;
});

const selectedFormat = computed<YtdlpFormatItem | null>(() => {
  if (!selectedFormatId.value) return null;
  return formatList.value.find((f) => f.formatId === selectedFormatId.value) ?? null;
});

const canConfirmDownload = computed(() => {
  return selectedFormatId.value !== null && downloadReady.value;
});
const canQueueBatch = computed(() => {
  if (!batchFiles.value.length) {
    return false;
  }

  if (batchDestinationMode.value === 'custom' && !batchOutputDir.value) {
    return false;
  }

  if (splitMode.value === 'manual') {
    return batchFiles.value.length === 1 && manualMarkers.value.length > 0;
  }

  return true;
});
const hasSavedCookiesFile = computed(() => Boolean(cookiesFile.value.trim()));
const queueStats = computed(() => getQueueStats(queue.value));
const visibleQueueItems = computed(() =>
  queueFilter.value === 'all'
    ? queue.value
    : queue.value.filter((item) => item.mediaKind === studioMode.value),
);
const queueHistoryCount = computed(() =>
  queue.value.filter((item) => isTerminalStatus(item.status)).length,
);
const batchSummary = computed(() => {
  if (!batchFiles.value.length) {
    return 'No source files selected';
  }

  if (batchFiles.value.length === 1) {
    return basename(batchFiles.value[0]);
  }

  return `${batchFiles.value.length} files selected`;
});
const batchOutputSummary = computed(() => {
  if (batchDestinationMode.value === 'custom') {
    return batchOutputDir.value || 'Choose a custom output folder';
  }

  if (batchFiles.value.length === 1) {
    return dirname(batchFiles.value[0]) || 'Use source folder';
  }

  return 'Each file keeps its own source folder';
});
const processSummary = computed(() => {
  const parts = [
    `Export ${batchFormat.value.toUpperCase()}`,
    batchDestinationMode.value === 'custom' ? 'Custom destination' : 'Output next to source files',
  ];

  if (isAudioBatch.value && normalizeAudio.value) {
    parts.push('Loudness normalize');
  }

  if (batchAudioTarget.value === 'azuracast' && isAudioBatch.value) {
    parts.push('AzuraCast Ready');
  }

  return parts.join(' • ');
});
const currentProcessModeLabel = computed(() => (isAudioStudio.value ? 'audio mastering' : 'video finishing'));
const processingToneSummary = computed(() => {
  if (!isAudioBatch.value) {
    return 'Video processing keeps export and trim controls only.';
  }

  const parts = [
    AUDIO_TARGETS.find((target) => target.value === batchAudioTarget.value)?.label ?? 'General audio',
  ];

  if (normalizeAudio.value) {
    parts.push(`Normalize ${loudnessTargetLufs.value} LUFS`);
  }

  if (
    Math.abs(eq.value.bass) > 0.01 ||
    Math.abs(eq.value.mid) > 0.01 ||
    Math.abs(eq.value.treble) > 0.01
  ) {
    parts.push(`EQ ${eq.value.bass}/${eq.value.mid}/${eq.value.treble} dB`);
  }

  if (fadeInDuration.value > 0 || fadeOutDuration.value > 0) {
    parts.push(`Fade ${fadeInDuration.value}s / ${fadeOutDuration.value}s`);
  }

  return parts.join(' • ');
});
const splitModeSummary = computed(
  () => SPLIT_MODES.find((mode) => mode.value === splitMode.value)?.hint ?? '',
);
const splitContextSummary = computed(() => {
  if (isAutoSplitMode.value) {
    return `Silence detect ${silence.value.thresholdDb} dB • ${silence.value.minSilenceDuration}s minimum gap • ${silence.value.minSegmentDuration}s minimum segment`;
  }

  if (isManualSplitMode.value) {
    return manualMarkers.value.length
      ? `${manualMarkers.value.length} manual mark(s) ready`
      : 'Manual mode selected. Add cut marks on the timeline.';
  }

  if (isChapterSplitMode.value) {
    return 'Use source chapters when present. If no valid chapter marks are found, silence detection is used.';
  }

  return 'Single trimmed output only. No extra split logic will be applied.';
});
const downloadSummaryRows = computed<SummaryRow[]>(() => {
  const rows: SummaryRow[] = [];

  if (selectedFormat.value) {
    const f = selectedFormat.value;
    rows.push({
      label: 'Format ID',
      value: f.formatId,
      detail: `${f.resolution || f.formatNote || 'Unknown resolution'} • ${f.ext.toUpperCase()}${f.vcodec ? ` • ${f.vcodec}` : ''}${f.acodec ? ` • ${f.acodec}` : ''}`,
    });
  }

  rows.push(
    { label: 'Export preset', value: downloadFormatSummary.value },
    {
      label: 'Playlist',
      value: PLAYLIST_MODES.find((mode) => mode.value === playlistMode.value)?.label ?? 'Auto detect',
      detail: currentPlaylistModeHint.value,
    },
    {
      label: 'Destination',
      value: resolvedDownloadPath.value,
      detail: downloadDestinationMode.value === 'custom' ? 'Custom folder' : 'System Downloads folder',
    },
    {
      label: 'Cookies',
      value: downloadAccessSummary.value,
      detail: 'Optional. Import a cookies.txt export only when the source needs an authenticated session.',
    },
  );

  return rows;
});
const processSummaryRows = computed<SummaryRow[]>(() => {
  const rows: SummaryRow[] = [
    { label: 'Sources', value: batchSummary.value },
    { label: 'Export', value: `Export ${batchFormat.value.toUpperCase()}` },
    { label: 'Output', value: batchOutputSummary.value },
  ];

  if (isAudioBatch.value) {
    rows.push({ label: 'Audio chain', value: processingToneSummary.value });
  }

  return rows;
});
const splitSummaryRows = computed<SummaryRow[]>(() => {
  const rows: SummaryRow[] = [
    {
      label: 'Mode',
      value: SPLIT_MODES.find((mode) => mode.value === splitMode.value)?.label ?? 'Trim only',
      detail: splitModeSummary.value,
    },
    {
      label: 'Trim',
      value: `${trimStart.value || '0'} -> ${trimEnd.value || 'end'}`,
    },
  ];

  if (isAutoSplitMode.value) {
    rows.push({
      label: 'Silence',
      value: `${silence.value.thresholdDb} dB • ${silence.value.minSilenceDuration}s`,
      detail: `Minimum segment ${silence.value.minSegmentDuration}s`,
    });
  } else if (isChapterSplitMode.value) {
    rows.push({
      label: 'Chapters',
      value: 'Prefer source chapter marks',
      detail: 'Falls back to silence detection when no chapters are embedded.',
    });
  } else if (isManualSplitMode.value) {
    rows.push({
      label: 'Markers',
      value: manualMarkers.value.length ? `${manualMarkers.value.length} mark(s)` : 'No manual marks yet',
      detail: splitContextSummary.value,
    });
  } else {
    rows.push({
      label: 'Output',
      value: 'Single trimmed output',
      detail: splitContextSummary.value,
    });
  }

  return rows;
});
const generateId = () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;

function formatBytes(bytes: number) {
  if (!bytes || bytes === 0) return 'Unknown size';
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${sizes[i]}`;
}

const addLog = (
  message: string,
  level: 'info' | 'warn' | 'error' | 'success' = 'info',
  scope: 'audio' | 'video' | 'system' = studioMode.value,
) => pushLog(message, level, scope);

const basename = (value: string) => value.split(/[/\\]/).pop() || value;
const dirname = (value: string) => {
  const normalized = value.replace(/\\/g, '/');
  const segments = normalized.split('/');
  segments.pop();
  return segments.join('/') || '';
};

function collectSettings(): PersistedStudioSettings {
  return {
    format: format.value,
    quality: quality.value,
    playlistMode: playlistMode.value,
    downloadAudioTarget: downloadAudioTarget.value,
    downloadVideoTarget: downloadVideoTarget.value,
    downloadDestinationMode: downloadDestinationMode.value,
    downloadPath: downloadPath.value,
    batchDestinationMode: batchDestinationMode.value,
    batchOutputDir: batchOutputDir.value,
    batchFormat: batchFormat.value,
    normalizeAudio: normalizeAudio.value,
    loudnessTargetLufs: loudnessTargetLufs.value,
    batchAudioTarget: batchAudioTarget.value,
    eq: eq.value,
    fadeInDuration: fadeInDuration.value,
    fadeOutDuration: fadeOutDuration.value,
    downloadPipelineSplitMode: downloadPipelineSplitMode.value,
    downloadPipelineSilence: downloadPipelineSilence.value,
  };
}

function persistSettings(mode: StudioMode = studioMode.value) {
  const settings = collectSettings();
  try {
    savePersistedSettings(mode, settings, studioMode.value);
  } catch (error) {
    addLog(`Failed to save settings: ${String(error)}`, 'error');
  }
}

function applySettings(settings: Record<string, unknown>, mode: StudioMode) {
  const next = sanitizeSettings(settings, mode);

  format.value = next.format;
  quality.value = next.quality;
  playlistMode.value = next.playlistMode;
  downloadAudioTarget.value = next.downloadAudioTarget;
  downloadVideoTarget.value = next.downloadVideoTarget;
  downloadDestinationMode.value = next.downloadDestinationMode;
  downloadPath.value = next.downloadPath;
  batchDestinationMode.value = next.batchDestinationMode;
  batchOutputDir.value = next.batchOutputDir;
  batchFormat.value = next.batchFormat;
  normalizeAudio.value = next.normalizeAudio;
  loudnessTargetLufs.value = next.loudnessTargetLufs;
  batchAudioTarget.value = next.batchAudioTarget;
  eq.value = next.eq;
  fadeInDuration.value = next.fadeInDuration;
  fadeOutDuration.value = next.fadeOutDuration;
  downloadPipelineSplitMode.value = next.downloadPipelineSplitMode;
  downloadPipelineSilence.value = next.downloadPipelineSilence;
}

function restoreSettings(mode: StudioMode = studioMode.value) {
  try {
    applySettings(restorePersistedSettings(mode), mode);
  } catch (error) {
    addLog(`Failed to restore settings: ${String(error)}`, 'error');
  }
}

function restoreActiveStudioMode() {
  try {
    const raw =
      window.localStorage.getItem(SETTINGS_STORAGE_KEY) ??
      window.localStorage.getItem(LEGACY_SETTINGS_STORAGE_KEY);
    if (!raw) {
      return;
    }
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (parsed.activeMode === 'audio' || parsed.activeMode === 'video') {
      studioMode.value = parsed.activeMode;
    }
  } catch {
    // Non-critical legacy settings payloads are ignored.
  }
}

function persistQueueHistory() {
  try {
    window.localStorage.setItem(QUEUE_STORAGE_KEY, JSON.stringify(queue.value));
  } catch (error) {
    addLog(`Failed to save queue history: ${String(error)}`, 'error');
  }
}

function scheduleQueuePersistence() {
  if (queuePersistenceTimer !== null) {
    window.clearTimeout(queuePersistenceTimer);
  }

  queuePersistenceTimer = window.setTimeout(() => {
    queuePersistenceTimer = null;
    persistQueueHistory();
  }, 180);
}

function restoreJobRequest(value: unknown): JobRequest | null {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const request = value as Record<string, unknown>;
  const restoreProcessPayload = (
    rawRequest: Record<string, unknown>,
    inputPath = '',
    outputDir?: string,
  ): ProcessJobRequest => {
    const rawEq = rawRequest.eq as Record<string, unknown> | undefined;
    const rawSilence = rawRequest.silence as Record<string, unknown> | undefined;
    const audioTarget: AudioTarget = rawRequest.audioTarget === 'azuracast' ? 'azuracast' : 'general';
    const splitMode: SplitMode =
      rawRequest.splitMode === 'silence' ||
      rawRequest.splitMode === 'chapters' ||
      rawRequest.splitMode === 'manual'
        ? rawRequest.splitMode
        : 'none';

    return {
      kind: 'process',
      inputPath,
      outputDir,
      format: typeof rawRequest.format === 'string' ? rawRequest.format : 'mp3',
      trimStart: typeof rawRequest.trimStart === 'string' ? rawRequest.trimStart : undefined,
      trimEnd: typeof rawRequest.trimEnd === 'string' ? rawRequest.trimEnd : undefined,
      normalize: Boolean(rawRequest.normalize),
      loudnessTargetLufs:
        typeof rawRequest.loudnessTargetLufs === 'number' ? rawRequest.loudnessTargetLufs : -16,
      eq: {
        bass: typeof rawEq?.bass === 'number' ? rawEq.bass : 0,
        mid: typeof rawEq?.mid === 'number' ? rawEq.mid : 0,
        treble: typeof rawEq?.treble === 'number' ? rawEq.treble : 0,
      },
      audioTarget,
      splitMode,
      manualMarkers: Array.isArray(rawRequest.manualMarkers)
        ? rawRequest.manualMarkers.filter((marker): marker is number => typeof marker === 'number')
        : [],
      silence: {
        thresholdDb: typeof rawSilence?.thresholdDb === 'number' ? rawSilence.thresholdDb : -35,
        minSilenceDuration:
          typeof rawSilence?.minSilenceDuration === 'number'
            ? rawSilence.minSilenceDuration
            : 1.5,
        minSegmentDuration:
          typeof rawSilence?.minSegmentDuration === 'number'
            ? rawSilence.minSegmentDuration
            : 20,
      },
      fadeInDuration: typeof rawRequest.fadeInDuration === 'number' ? rawRequest.fadeInDuration : 0,
      fadeOutDuration: typeof rawRequest.fadeOutDuration === 'number' ? rawRequest.fadeOutDuration : 0,
    };
  };

  if (request.kind === 'download') {
    const playlistMode: PlaylistMode =
      request.playlistMode === 'playlist' || request.playlistMode === 'single'
        ? request.playlistMode
        : 'auto';
    const audioTarget: AudioTarget = request.audioTarget === 'azuracast' ? 'azuracast' : 'general';
    const videoTarget: VideoTarget =
      request.videoTarget === 'google_tv_cast' ? 'google_tv_cast' : 'general';

    return {
      kind: 'download',
      url: typeof request.url === 'string' ? request.url : '',
      format: typeof request.format === 'string' ? request.format : 'mp3',
      quality: typeof request.quality === 'string' ? request.quality : 'best',
      outputPath: typeof request.outputPath === 'string' ? request.outputPath : '',
      playlistMode,
      audioTarget,
      videoTarget,
      cookiesFile: typeof request.cookiesFile === 'string' ? request.cookiesFile : undefined,
    };
  }

  if (request.kind === 'process') {
    return restoreProcessPayload(
      request,
      typeof request.inputPath === 'string' ? request.inputPath : '',
      typeof request.outputDir === 'string' ? request.outputDir : undefined,
    );
  }

  if (request.kind === 'downloadProcess') {
    const rawProcess = request.process as Record<string, unknown> | undefined;
    const playlistMode: PlaylistMode =
      request.playlistMode === 'playlist' || request.playlistMode === 'single'
        ? request.playlistMode
        : 'auto';

    return {
      kind: 'downloadProcess',
      url: typeof request.url === 'string' ? request.url : '',
      downloadFormat: typeof request.downloadFormat === 'string' ? request.downloadFormat : 'mp3',
      downloadQuality: typeof request.downloadQuality === 'string' ? request.downloadQuality : 'best',
      outputDir: typeof request.outputDir === 'string' ? request.outputDir : '',
      playlistMode,
      audioTarget: request.audioTarget === 'azuracast' ? 'azuracast' : 'general',
      videoTarget: request.videoTarget === 'google_tv_cast' ? 'google_tv_cast' : 'general',
      cookiesFile: typeof request.cookiesFile === 'string' ? request.cookiesFile : undefined,
      process: restoreProcessPayload(rawProcess ?? {}),
      cleanupTemp: request.cleanupTemp !== false,
    };
  }

  return null;
}

function restoreQueueItem(value: unknown): QueueItemData | null {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const item = value as Record<string, unknown>;
  const request = restoreJobRequest(item.request);
  if (!request) {
    return null;
  }

  const restoredStatus =
    item.status === 'waiting'
      ? 'cancelled'
      : isActiveStatus(String(item.status) as QueueStatus)
        ? 'cancelled'
        : item.status === 'done' || item.status === 'error' || item.status === 'cancelled'
          ? item.status
          : 'cancelled';

  const restartedDetail =
    item.status === 'waiting'
      ? 'Pending in a previous session and not started.'
      : isActiveStatus(String(item.status) as QueueStatus)
        ? 'Interrupted when MediaToolsPro was reloaded.'
        : typeof item.detail === 'string'
          ? item.detail
          : undefined;

  return {
    id: typeof item.id === 'string' ? item.id : generateId(),
    kind:
      request.kind === 'downloadProcess'
        ? 'pipeline'
        : request.kind === 'process'
          ? 'process'
          : 'download',
    mediaKind: item.mediaKind === 'video' ? 'video' : 'audio',
    format:
      typeof item.format === 'string'
        ? item.format
        : request.kind === 'downloadProcess'
          ? request.process.format
          : request.format,
    quality: typeof item.quality === 'string' ? item.quality : undefined,
    status: restoredStatus,
    percent: typeof item.percent === 'number' ? item.percent : 0,
    speed: isTerminalStatus(restoredStatus) ? '-' : typeof item.speed === 'string' ? item.speed : '-',
    eta: isTerminalStatus(restoredStatus) ? '-' : typeof item.eta === 'string' ? item.eta : '-',
    totalSize: typeof item.totalSize === 'string' ? item.totalSize : '-',
    title: typeof item.title === 'string' ? item.title : '',
    source:
      typeof item.source === 'string'
        ? item.source
        : request.kind === 'download'
          ? request.url
          : request.kind === 'downloadProcess'
            ? request.url
            : request.inputPath,
    detail: restartedDetail,
    outputPath: typeof item.outputPath === 'string' ? item.outputPath : undefined,
    error: typeof item.error === 'string' ? item.error : undefined,
    request,
    pipelineStage: item.pipelineStage === 'process' ? 'process' : undefined,
    tempDir: typeof item.tempDir === 'string' ? item.tempDir : undefined,
    downloadedPath: typeof item.downloadedPath === 'string' ? item.downloadedPath : undefined,
  };
}

function restoreQueueHistory() {
  try {
    const raw =
      window.localStorage.getItem(QUEUE_STORAGE_KEY) ??
      window.localStorage.getItem(LEGACY_QUEUE_STORAGE_KEY);
    if (!raw) {
      return;
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return;
    }

    const restored = parsed
      .map((entry) => restoreQueueItem(entry))
      .filter((entry): entry is QueueItemData => Boolean(entry));

    if (!restored.length) {
      return;
    }

    const unfinishedCount = restored.filter((item) =>
      item.detail === 'Pending in a previous session and not started.' ||
      item.detail === 'Interrupted when MediaToolsPro was reloaded.' ||
      item.detail === 'Interrupted when AudioToolsPro was reloaded.',
    ).length;

    queue.value = restored;
    addLog(`Restored ${restored.length} queue item${restored.length === 1 ? '' : 's'} from the previous session.`, 'info');

    if (unfinishedCount > 0) {
      addLog(
        `${unfinishedCount} unfinished job${unfinishedCount === 1 ? '' : 's'} were marked as cancelled after reload.`,
        'warn',
      );
    }
  } catch (error) {
    addLog(`Failed to restore queue history: ${String(error)}`, 'error');
  }
}

function clearQueueHistory() {
  const removed = queue.value.filter((item) => isTerminalStatus(item.status)).length;
  if (!removed) {
    return;
  }

  queue.value = queue.value.filter((item) => !isTerminalStatus(item.status));
  addLog(`Cleared ${removed} past job${removed === 1 ? '' : 's'} from queue history.`, 'info');
}

function markerPosition(seconds: number) {
  if (currentDuration.value <= 0) {
    return 0;
  }

  return Math.min(100, Math.max(0, (seconds / currentDuration.value) * 100));
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function formatDisplayTime(value: number) {
  const safe = Math.max(0, value);
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;

  const secondLabel =
    Math.abs(seconds - Math.round(seconds)) > 0.05
      ? seconds.toFixed(1).padStart(4, '0')
      : `${Math.round(seconds)}`.padStart(2, '0');

  if (hours > 0) {
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${secondLabel}`;
  }

  return `${String(minutes).padStart(2, '0')}:${secondLabel}`;
}

function parseTimeInput(value: string) {
  const raw = value.trim();
  if (!raw) {
    return null;
  }

  if (!raw.includes(':')) {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : null;
  }

  const parts = raw.split(':').map((part) => Number(part));
  if (parts.some((part) => Number.isNaN(part))) {
    return null;
  }

  let seconds = 0;
  for (const part of parts) {
    seconds = seconds * 60 + part;
  }

  return seconds;
}

function normalizeTrimBounds(
  duration: number,
  start: number,
  end: number,
  changed: TrimHandle | 'none' = 'none',
) {
  const safeDuration = Math.max(0, duration);
  if (safeDuration <= 0) {
    return { start: 0, end: 0 };
  }

  const gap = Math.min(MIN_TRIM_GAP_SECONDS, safeDuration);
  let nextStart = clamp(start, 0, safeDuration);
  let nextEnd = clamp(end, 0, safeDuration);

  if (nextEnd < nextStart) {
    if (changed === 'start') {
      nextStart = Math.max(0, nextEnd - gap);
    } else {
      nextEnd = Math.min(safeDuration, nextStart + gap);
    }
  }

  if (nextEnd - nextStart < gap) {
    if (changed === 'start') {
      nextStart = Math.max(0, nextEnd - gap);
    } else {
      nextEnd = Math.min(safeDuration, nextStart + gap);
      if (nextEnd - nextStart < gap) {
        nextStart = Math.max(0, nextEnd - gap);
      }
    }
  }

  return { start: nextStart, end: nextEnd };
}

function syncTrimInputs(start: number, end: number) {
  const duration = currentDuration.value;
  trimStart.value = start <= 0.05 ? '' : formatDisplayTime(start);
  trimEnd.value = duration > 0 && end >= duration - 0.05 ? '' : formatDisplayTime(end);
}

function setTrimBoundary(handle: TrimHandle, seconds: number) {
  const current = trimSelection.value;
  const next =
    handle === 'start'
      ? normalizeTrimBounds(currentDuration.value, seconds, current.end, 'start')
      : normalizeTrimBounds(currentDuration.value, current.start, seconds, 'end');

  syncTrimInputs(next.start, next.end);
}

function resetTrimSelection() {
  trimStart.value = '';
  trimEnd.value = '';
}

function getActiveCount(kind: QueueItemData['kind']) {
  return queue.value.filter((item) => item.kind === kind && isActiveStatus(item.status)).length;
}

function getBackendJobId(item: QueueItemData) {
  return item.activeBackendId ?? item.id;
}

function buildDownloadQualityOptions() {
  if (isAudioDownload.value) {
    return ['best', ...AUDIO_BITRATES];
  }

  return ['best', ...VIDEO_QUALITIES];
}

function buildDownloadDetail(request: DownloadJobRequest) {
  const parts = [
    PLAYLIST_MODES.find((mode) => mode.value === request.playlistMode)?.label ?? 'Auto detect',
    request.formatId ? `Format ID: ${request.formatId}` : `${request.format.toUpperCase()} ${request.quality}`,
    `Output: ${request.outputPath}`,
  ];

  if (request.cookiesFile) {
    parts.push('Cookies file: saved internally');
  }

  return parts.join(' • ');
}

async function restoreSavedCookiesFile() {
  try {
    const saved = await invoke<string | null>('get_saved_cookies_file');
    cookiesFile.value = saved ?? '';
  } catch (error) {
    addLog(`Failed to restore saved cookies.txt: ${String(error)}`, 'error');
  }
}

async function clearSavedCookiesFile() {
  try {
    await invoke('clear_saved_cookies_file');
    cookiesFile.value = '';
    addLog('Saved internal cookies.txt removed.', 'info');
  } catch (error) {
    addLog(`Failed to remove saved cookies.txt: ${String(error)}`, 'error');
  }
}

async function ensureSavedCookiesFileAvailable() {
  try {
    const saved = await invoke<string | null>('get_saved_cookies_file');
    if (saved) {
      cookiesFile.value = saved;
      return true;
    }
  } catch (error) {
    addLog(`Failed to verify saved cookies.txt: ${String(error)}`, 'error');
    return false;
  }

  cookiesFile.value = '';
  addLog(
    'The internally saved cookies.txt is no longer available. Import a fresh cookies.txt before downloading protected content.',
    'warn',
  );
  return false;
}

function shouldWarnCookiesFileFailure(message: string) {
  return /cookies\.txt may be expired|export a fresh cookies\.txt|saved internal cookies\.txt/i.test(
    message,
  );
}

function buildProcessDetail(request: ProcessJobRequest) {
  const parts: string[] = [];

  if (request.trimStart || request.trimEnd) {
    parts.push(`Trim ${request.trimStart || '0'} -> ${request.trimEnd || 'end'}`);
  }

  parts.push(request.outputDir ? `Output: ${request.outputDir}` : 'Output: source folder');

  if (request.audioTarget === 'azuracast') {
    parts.push('AzuraCast Ready');
  }

  if (request.splitMode === 'silence') {
    parts.push(
      `Silence split @ ${request.silence.thresholdDb} dB / ${request.silence.minSilenceDuration}s`,
    );
  } else if (request.splitMode === 'chapters') {
    parts.push('Source chapters split with silence fallback');
  } else if (request.splitMode === 'manual') {
    parts.push(`Manual split (${request.manualMarkers.length} cuts)`);
  }

  if (request.normalize) {
    parts.push(`Loudness normalize ${request.loudnessTargetLufs} LUFS`);
  }

  if (
    Math.abs(request.eq.bass) > 0.01 ||
    Math.abs(request.eq.mid) > 0.01 ||
    Math.abs(request.eq.treble) > 0.01
  ) {
    parts.push(`EQ B${request.eq.bass} / M${request.eq.mid} / T${request.eq.treble} dB`);
  }

  if (request.fadeInDuration > 0 || request.fadeOutDuration > 0) {
    parts.push(`Fade in ${request.fadeInDuration}s / out ${request.fadeOutDuration}s`);
  }

  return parts.join(' • ') || 'Transcode only';
}

function processQueue() {
  while (getActiveCount('download') + getActiveCount('pipeline') < DOWNLOAD_CONCURRENT_LIMIT) {
    const nextDownload = queue.value.find(
      (item) => (item.kind === 'download' || item.kind === 'pipeline') && item.status === 'waiting',
    );
    if (!nextDownload) {
      break;
    }
    if (nextDownload.kind === 'pipeline') {
      void startPipeline(nextDownload);
    } else {
      void startDownload(nextDownload);
    }
  }

  while (getActiveCount('process') < PROCESS_CONCURRENT_LIMIT) {
    const nextProcess = queue.value.find(
      (item) => item.kind === 'process' && item.status === 'waiting',
    );
    if (!nextProcess) {
      break;
    }
    void startProcess(nextProcess);
  }
}

async function startDownload(item: QueueItemData) {
  if (item.request.kind !== 'download') {
    return;
  }

  item.status = 'downloading';
  item.percent = 0;

  try {
    await invoke('start_download', {
      request: {
        ...item.request,
        id: item.id,
      },
    });
    addLog(`Starting download: ${item.request.url}`, 'info', item.mediaKind);
  } catch (error) {
    const message = String(error);
    item.status = 'error';
    item.error = message;
    item.detail = message;
    addLog(`Failed to start download: ${message}`, 'error', item.mediaKind);
    processQueue();
  }
}

async function startPipeline(item: QueueItemData) {
  if (item.request.kind !== 'downloadProcess') {
    return;
  }

  item.status = 'downloading';
  item.pipelineStage = 'download';
  item.percent = 0;
  item.detail = 'Downloading to a temporary workspace';

  try {
    const tempDir = await invoke<string>('prepare_pipeline_temp_dir', { id: item.id });
    item.tempDir = tempDir;
    item.activeBackendId = `${item.id}-download`;

    await invoke('start_download', {
      request: {
        id: item.activeBackendId,
        url: item.request.url,
        format: item.request.downloadFormat,
        quality: item.request.downloadQuality,
        outputPath: tempDir,
        playlistMode: item.request.playlistMode,
        audioTarget: item.request.audioTarget,
        videoTarget: item.request.videoTarget,
        cookiesFile: item.request.cookiesFile,
      },
    });
    addLog(`Starting download/process pipeline: ${item.request.url}`, 'info', item.mediaKind);
  } catch (error) {
    const message = String(error);
    item.status = 'error';
    item.error = message;
    item.detail = message;
    addLog(`Failed to start pipeline: ${message}`, 'error', item.mediaKind);
    processQueue();
  }
}

async function startPipelineProcess(item: QueueItemData) {
  if (item.request.kind !== 'downloadProcess' || !item.downloadedPath) {
    return;
  }

  item.status = 'processing';
  item.pipelineStage = 'process';
  item.percent = Math.max(item.percent, 50);
  item.activeBackendId = `${item.id}-process`;
  item.detail = 'Download finished. Processing final output.';

  try {
    await invoke('start_media_process', {
      request: {
        ...item.request.process,
        id: item.activeBackendId,
        inputPath: item.downloadedPath,
        outputDir: item.request.outputDir,
      },
    });
  } catch (error) {
    const message = String(error);
    item.status = 'error';
    item.error = message;
    item.detail = message;
    addLog(`Failed to start pipeline processing: ${message}`, 'error', item.mediaKind);
    await cleanupPipelineTemp(item);
    processQueue();
  }
}

async function startProcess(item: QueueItemData) {
  if (item.request.kind !== 'process') {
    return;
  }

  item.status = 'processing';
  item.percent = 0;

  try {
    await invoke('start_media_process', {
      request: {
        ...item.request,
        id: item.id,
      },
    });
    addLog(`Starting media processing: ${item.title}`, 'info', item.mediaKind);
  } catch (error) {
    const message = String(error);
    item.status = 'error';
    item.error = message;
    item.detail = message;
    addLog(`Failed to start processing ${item.title}: ${message}`, 'error', item.mediaKind);
    processQueue();
  }
}

async function cleanupPipelineTemp(item: QueueItemData) {
  if (item.request.kind !== 'downloadProcess' || !item.request.cleanupTemp || !item.tempDir) {
    return;
  }

  try {
    await invoke('cleanup_pipeline_temp_dir', { path: item.tempDir });
    item.tempDir = undefined;
  } catch (error) {
    addLog(`Pipeline temp cleanup failed: ${String(error)}`, 'warn');
  }
}

function buildCurrentProcessSettings(
  context: 'local' | 'pipeline' = 'local',
): Omit<ProcessJobRequest, 'kind' | 'inputPath' | 'outputDir'> {
  const effectiveAudioTarget: AudioTarget =
    batchAudioTarget.value === 'azuracast' && isAudioBatch.value ? 'azuracast' : 'general';
  const effectiveFormat = effectiveAudioTarget === 'azuracast' ? 'mp3' : batchFormat.value;
  const effectiveNormalize = effectiveAudioTarget === 'azuracast' ? true : normalizeAudio.value;
  const effectiveSplitMode = context === 'pipeline' ? downloadPipelineSplitMode.value : splitMode.value;
  const effectiveSilence = context === 'pipeline' ? downloadPipelineSilence.value : silence.value;

  return {
    format: effectiveFormat,
    trimStart: context === 'local' ? trimStart.value.trim() || undefined : undefined,
    trimEnd: context === 'local' ? trimEnd.value.trim() || undefined : undefined,
    normalize: effectiveNormalize,
    loudnessTargetLufs: loudnessTargetLufs.value,
    eq: { ...eq.value },
    audioTarget: effectiveAudioTarget,
    splitMode: effectiveSplitMode,
    manualMarkers: context === 'local' ? [...manualMarkers.value] : [],
    silence: { ...effectiveSilence },
    fadeInDuration: fadeInDuration.value,
    fadeOutDuration: fadeOutDuration.value,
  };
}

function buildCurrentDownloadSettings(outputPath: string): Omit<DownloadJobRequest, 'kind' | 'url'> {
  return {
    format: format.value,
    quality: quality.value,
    formatId: selectedFormatId.value ?? undefined,
    outputPath,
    playlistMode: playlistMode.value,
    audioTarget: 'general',
    videoTarget: 'general',
    cookiesFile: cookiesFile.value.trim() || undefined,
  };
}

async function exploreFormats() {
  const trimmedUrl = url.value.trim();
  if (!trimmedUrl) return;

  formatListLoading.value = true;
  formatListError.value = '';
  formatList.value = [];
  selectedFormatId.value = null;

    try {
      const response = await invoke<YtdlpFormatsResponse>('list_formats', {
        request: {
          url: trimmedUrl,
          cookiesFile: cookiesFile.value.trim() || undefined,
        },
      });
    formatList.value = response.formats;
    formatListTitle.value = response.title;
    downloadStep.value = 'formats';
    addLog(`Explored formats for: ${trimmedUrl} (${response.formats.length} formats)`, 'info');
  } catch (error) {
    const message = String(error);
    formatListError.value = message;
    addLog(`Failed to list formats: ${message}`, 'error');
  } finally {
    formatListLoading.value = false;
  }
}

function resetFormatExplorer() {
  downloadStep.value = 'url';
  formatList.value = [];
  selectedFormatId.value = null;
  formatListError.value = '';
  formatListTitle.value = '';
}

async function addDownloadToQueue() {
  const trimmedUrl = url.value.trim();
  if (!trimmedUrl) {
    return;
  }

  if (downloadDestinationMode.value === 'custom' && !downloadPath.value) {
    addLog('Choose a custom download folder or switch back to the Downloads folder.', 'warn');
    return;
  }

  if (cookiesFile.value.trim()) {
    const savedCookiesAvailable = await ensureSavedCookiesFileAvailable();
    if (!savedCookiesAvailable) {
      return;
    }
  }

  const downloadSettings = buildCurrentDownloadSettings(resolvedDownloadPath.value);

  const request: DownloadJobRequest = {
    kind: 'download',
    url: trimmedUrl,
    ...downloadSettings,
  };

  queue.value.unshift({
    id: generateId(),
    kind: 'download',
    mediaKind: AUDIO_FORMATS.includes(request.format) ? 'audio' : 'video',
    format: request.format,
    quality: request.quality,
    status: 'waiting',
    percent: 0,
    speed: '-',
    eta: '-',
    totalSize: '-',
    title: trimmedUrl,
    source: trimmedUrl,
    detail: buildDownloadDetail(request),
    request,
  });

  addLog(`Added download job: ${trimmedUrl}`, 'info');
  url.value = '';
  resetFormatExplorer();
  activeTab.value = 'queue';
  processQueue();
}

function addBatchToQueue() {
  if (!batchFiles.value.length) {
    return;
  }

  if (batchDestinationMode.value === 'custom' && !batchOutputDir.value) {
    addLog('Choose a custom output folder or keep the source-folder destination.', 'warn');
    return;
  }

  if (splitMode.value === 'manual' && batchFiles.value.length !== 1) {
    addLog('Manual split currently works with a single source file at a time.', 'warn');
    return;
  }

  if (splitMode.value === 'manual' && manualMarkers.value.length === 0) {
    addLog('Add at least one manual mark before queueing a manual split.', 'warn');
    return;
  }

  const processSettings = buildCurrentProcessSettings();

  for (const inputPath of batchFiles.value) {
    const request: ProcessJobRequest = {
      kind: 'process',
      inputPath,
      outputDir: batchDestinationMode.value === 'custom' ? batchOutputDir.value : undefined,
      ...processSettings,
    };

    queue.value.unshift({
      id: generateId(),
      kind: 'process',
      mediaKind: isAudioBatch.value ? 'audio' : 'video',
      format: request.format,
      status: 'waiting',
      percent: 0,
      speed: '-',
      eta: '-',
      totalSize: '-',
      title: basename(inputPath),
      source: inputPath,
      detail: buildProcessDetail(request),
      request,
    });
  }

  addLog(`Added ${batchFiles.value.length} files to the processing queue`, 'info');

  if (splitMode.value !== 'manual') {
    batchFiles.value = [];
    clearSingleFilePreview();
  }

  activeTab.value = 'queue';
  processQueue();
}

async function cancelItem(id: string) {
  const item = queue.value.find((entry) => entry.id === id);
  if (!item) {
    return;
  }

  if (item.status === 'waiting') {
    queue.value = queue.value.filter((entry) => entry.id !== id);
    addLog(`Removed queued job: ${item.title}`, 'info');
    return;
  }

  if (isActiveStatus(item.status)) {
    item.status = 'cancelled';
    item.detail = 'Cancellation requested';

    try {
      await invoke('cancel_job', { id: getBackendJobId(item) });
      addLog(`Cancelled job: ${item.title}`, 'warn', item.mediaKind);
      await cleanupPipelineTemp(item);
    } catch (error) {
      const message = String(error);
      item.status = 'error';
      item.error = message;
      item.detail = message;
      addLog(`Failed to cancel ${item.title}: ${message}`, 'error', item.mediaKind);
    }

    processQueue();
    return;
  }

  queue.value = queue.value.filter((entry) => entry.id !== id);
}

async function revealQueueOutput(path: string) {
  try {
    await invoke('reveal_path', { path });
  } catch (error) {
    addLog(`Failed to open output folder: ${String(error)}`, 'error');
  }
}

function sendToProcess(path: string) {
  batchFiles.value = [path];
  activeTab.value = 'process';
  addLog(`Sent ${basename(path)} to Process`, 'info');
}

function updateQueueItem(payload: JobProgressPayload) {
  const item = queue.value.find(
    (entry) => entry.id === payload.id || entry.activeBackendId === payload.id,
  );
  if (!item) {
    return;
  }

  if (item.kind === 'pipeline' && item.request.kind === 'downloadProcess') {
    void updatePipelineQueueItem(item, payload);
    return;
  }

  const previousStatus = item.status;
  applyJobProgress(item, payload);

  if (payload.status !== previousStatus) {
    if (payload.status === 'done') {
      addLog(
        `Finished ${item.kind === 'download' ? 'download' : 'processing'}: ${item.title}`,
        'success',
        item.mediaKind,
      );
    } else if (payload.status === 'error') {
      addLog(`Job failed: ${item.title}${payload.error ? ` (${payload.error})` : ''}`, 'error', item.mediaKind);
      if (
        item.kind === 'download' &&
        item.request.kind === 'download' &&
        item.request.cookiesFile &&
        payload.error &&
        shouldWarnCookiesFileFailure(payload.error)
      ) {
        addLog(
          'The saved internal cookies.txt looks outdated. Export a fresh cookies.txt and import it again.',
          'warn',
        );
      }
    } else if (payload.status === 'cancelled') {
      addLog(`Job cancelled: ${item.title}`, 'warn', item.mediaKind);
    }
  }

  if (['done', 'error', 'cancelled'].includes(payload.status)) {
    processQueue();
  }
}

async function updatePipelineQueueItem(item: QueueItemData, payload: JobProgressPayload) {
  const previousStatus = item.status;
  const stage = item.pipelineStage ?? 'download';

  item.speed = payload.speed;
  item.eta = payload.eta;
  item.totalSize = payload.totalSize;

  if (payload.title) {
    item.title = payload.title;
  }

  if (payload.detail) {
    item.detail = payload.detail;
  }

  if (payload.error) {
    item.error = payload.error;
  }

  if (stage === 'download') {
    item.status = payload.status === 'error' || payload.status === 'cancelled' ? payload.status : 'downloading';
    item.percent = Math.min(50, Math.max(0, payload.percent * 0.5));

    if (payload.outputPath) {
      item.downloadedPath = payload.outputPath;
    }

    if (payload.status === 'done') {
      if (!item.downloadedPath) {
        item.status = 'error';
        item.error = 'Download finished but no output file path was reported.';
        item.detail = item.error;
        await cleanupPipelineTemp(item);
        processQueue();
        return;
      }

      await startPipelineProcess(item);
      return;
    }
  } else {
    item.status = payload.status;
    item.percent = payload.status === 'done' ? 100 : 50 + Math.min(50, Math.max(0, payload.percent * 0.5));

    if (payload.outputPath) {
      item.outputPath = payload.outputPath;
    }
  }

  if (payload.status !== previousStatus) {
    if (payload.status === 'done' && stage === 'process') {
      addLog(`Finished pipeline: ${item.title}`, 'success', item.mediaKind);
      await cleanupPipelineTemp(item);
    } else if (payload.status === 'error') {
      addLog(`Pipeline failed: ${item.title}${payload.error ? ` (${payload.error})` : ''}`, 'error', item.mediaKind);
      await cleanupPipelineTemp(item);
    } else if (payload.status === 'cancelled') {
      addLog(`Pipeline cancelled: ${item.title}`, 'warn', item.mediaKind);
      await cleanupPipelineTemp(item);
    }
  }

  if (['done', 'error', 'cancelled'].includes(payload.status)) {
    processQueue();
  }
}

async function selectOutputFolder(target: 'download' | 'batch') {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title:
        target === 'download' ? 'Select download folder' : 'Select custom output folder',
    });

    if (selected && typeof selected === 'string') {
      if (target === 'download') {
        downloadPath.value = selected;
        downloadDestinationMode.value = 'custom';
      } else {
        batchOutputDir.value = selected;
        batchDestinationMode.value = 'custom';
      }

      addLog(`Output folder set to: ${selected}`, 'info');
    }
  } catch (error) {
    addLog(`Failed to open folder dialog: ${String(error)}`, 'error');
  }
}

async function selectCookiesFile() {
  try {
    const selected = await open({
      directory: false,
      multiple: false,
      title: 'Import cookies.txt',
      filters: [{ name: 'Cookies', extensions: ['txt'] }],
    });

    if (selected && typeof selected === 'string') {
      const savedPath = await invoke<string>('import_cookies_file', { sourcePath: selected });
      cookiesFile.value = savedPath;
      addLog(`Cookies file imported into internal app storage from ${basename(selected)}`, 'info');
    }
  } catch (error) {
    addLog(`Failed to choose or import cookies.txt: ${String(error)}`, 'error');
  }
}

async function loadMediaProbe(filePath: string) {
  try {
    mediaProbe.value = await invoke<MediaProbeResponse>('probe_media', {
      inputPath: filePath,
    });
    timelineCursor.value = clamp(timelineCursor.value, 0, mediaProbe.value.durationSeconds);
    const current = normalizeTrimBounds(
      mediaProbe.value.durationSeconds,
      parseTimeInput(trimStart.value) ?? 0,
      parseTimeInput(trimEnd.value) ?? mediaProbe.value.durationSeconds,
    );
    syncTrimInputs(current.start, current.end);
  } catch (error) {
    mediaProbe.value = null;
    addLog(`Failed to inspect media file ${basename(filePath)}: ${String(error)}`, 'error');
  }
}

async function loadWaveformPreview(filePath: string) {
  const currentToken = ++waveformRequestToken;
  waveformLoading.value = true;
  waveformError.value = '';
  waveformPreview.value = null;

  try {
    const preview = await invoke<WaveformPreviewResponse>('render_waveform_preview', {
      inputPath: filePath,
    });

    if (currentToken !== waveformRequestToken) {
      return;
    }

    waveformPreview.value = preview;
  } catch (error) {
    if (currentToken !== waveformRequestToken) {
      return;
    }

    waveformError.value = String(error);
    addLog(`Waveform preview unavailable for ${basename(filePath)}: ${String(error)}`, 'warn');
  } finally {
    if (currentToken === waveformRequestToken) {
      waveformLoading.value = false;
    }
  }
}

async function refreshSelectedFile(filePath: string) {
  await loadMediaProbe(filePath);
  await loadWaveformPreview(filePath);
}

function clearSingleFilePreview() {
  waveformRequestToken += 1;
  if (audioPreviewElement.value) {
    audioPreviewElement.value.pause();
    audioPreviewElement.value.removeAttribute('src');
    audioPreviewElement.value.load();
  }
  mediaProbe.value = null;
  waveformPreview.value = null;
  waveformError.value = '';
  waveformLoading.value = false;
  timelineCursor.value = 0;
  manualMarkers.value = [];
  isPreviewPlaying.value = false;
}

async function selectBatchFiles() {
  try {
    const selected = await open({
      directory: false,
      multiple: true,
      title: 'Select audio or video files',
    });

    if (Array.isArray(selected)) {
      batchFiles.value = selected;
    } else if (typeof selected === 'string') {
      batchFiles.value = [selected];
    } else {
      return;
    }

    manualMarkers.value = [];
    timelineCursor.value = 0;

    if (batchFiles.value.length === 1) {
      await refreshSelectedFile(batchFiles.value[0]);
    } else {
      clearSingleFilePreview();
    }

    if (batchFiles.value.length) {
      addLog(`Selected ${batchFiles.value.length} files for processing`, 'info');
    }
  } catch (error) {
    addLog(`Failed to pick batch files: ${String(error)}`, 'error');
  }
}

function addManualMarker(seconds: number) {
  if (!currentDuration.value) {
    return;
  }

  const clamped = clamp(seconds, 0, currentDuration.value);
  if (clamped <= 0 || clamped >= currentDuration.value) {
    addLog('Manual marks must stay inside the file duration.', 'warn');
    return;
  }

  if (manualMarkers.value.some((marker) => Math.abs(marker - clamped) < 0.2)) {
    addLog('There is already a manual mark very close to that position.', 'warn');
    return;
  }

  manualMarkers.value = [...manualMarkers.value, clamped].sort((left, right) => left - right);
}

function setTimelineCursorFromSeconds(seconds: number) {
  const next = clamp(seconds, 0, currentDuration.value);
  timelineCursor.value = next;

  if (audioPreviewElement.value) {
    audioPreviewElement.value.currentTime = next;
  }
}

function resolveSecondsFromClientX(clientX: number) {
  if (!waveformTrack.value || currentDuration.value <= 0) {
    return 0;
  }

  const rect = waveformTrack.value.getBoundingClientRect();
  const ratio = clamp((clientX - rect.left) / rect.width, 0, 1);
  return ratio * currentDuration.value;
}

function handleWaveformClick(event: MouseEvent) {
  if (activeTrimHandle.value) {
    return;
  }

  setTimelineCursorFromSeconds(resolveSecondsFromClientX(event.clientX));
}

function beginTrimDrag(handle: TrimHandle, event: PointerEvent) {
  if (!currentDuration.value) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  activeTrimHandle.value = handle;

  const move = (moveEvent: PointerEvent) => {
    const nextSeconds = resolveSecondsFromClientX(moveEvent.clientX);
    setTrimBoundary(handle, nextSeconds);
    setTimelineCursorFromSeconds(nextSeconds);
  };

  const stop = () => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', stop);
    window.removeEventListener('pointercancel', stop);
    activeTrimHandle.value = null;
    stopPointerTracking = null;
  };

  stopPointerTracking?.();
  stopPointerTracking = stop;

  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', stop);
  window.addEventListener('pointercancel', stop);
  move(event);
}

function setTrimFromCursor(handle: TrimHandle) {
  setTrimBoundary(handle, timelineCursor.value);
}

function openOverlay(panel: OverlayPanel) {
  activeOverlay.value = panel;
}

function closeOverlay() {
  activeOverlay.value = null;
}

function addMarkerAtCursor() {
  addManualMarker(timelineCursor.value);
}

function addMarkerFromInput() {
  const seconds = parseTimeInput(manualMarkerInput.value);

  if (seconds === null) {
    addLog('Use a valid time such as 95.5 or 01:35.5 for manual marks.', 'warn');
    return;
  }

  addManualMarker(seconds);
  manualMarkerInput.value = '';
}

function removeMarker(marker: number) {
  manualMarkers.value = manualMarkers.value.filter((value) => Math.abs(value - marker) > 0.05);
}

function handleCursorSlider(event: Event) {
  const target = event.target as HTMLInputElement | null;
  if (!target) {
    return;
  }

  setTimelineCursorFromSeconds(Number(target.value));
}

function seekPreview(deltaSeconds: number) {
  if (!audioPreviewElement.value) {
    return;
  }

  setTimelineCursorFromSeconds(audioPreviewElement.value.currentTime + deltaSeconds);
}

async function togglePreviewPlayback() {
  const audio = audioPreviewElement.value;
  if (!audio || !audioPreviewUrl.value) {
    addLog('Load a source file first to preview the audio while editing.', 'warn');
    return;
  }

  if (audio.paused) {
    const { start, end } = trimSelection.value;
    if (audio.currentTime < start || audio.currentTime > end) {
      audio.currentTime = start;
      timelineCursor.value = start;
    }

    try {
      await audio.play();
      isPreviewPlaying.value = true;
    } catch (error) {
      addLog(`Audio preview could not start: ${String(error)}`, 'error');
    }
    return;
  }

  audio.pause();
  isPreviewPlaying.value = false;
}

function onPreviewTimeUpdate() {
  const audio = audioPreviewElement.value;
  if (!audio) {
    return;
  }

  timelineCursor.value = clamp(audio.currentTime, 0, currentDuration.value);
  const { start, end } = trimSelection.value;
  const hasExplicitSelection = Boolean(trimStart.value.trim() || trimEnd.value.trim());
  const reachedSelectionEnd = end > start + MIN_TRIM_GAP_SECONDS && audio.currentTime >= end - 0.03;

  if (!reachedSelectionEnd) {
    return;
  }

  if (loopSelection.value) {
    audio.currentTime = start;
    timelineCursor.value = start;
    void audio.play();
    return;
  }

  if (hasExplicitSelection) {
    audio.pause();
    audio.currentTime = end;
    timelineCursor.value = end;
    isPreviewPlaying.value = false;
  }
}

function onPreviewPlay() {
  isPreviewPlaying.value = true;
}

function onPreviewPause() {
  isPreviewPlaying.value = false;
}

function onPreviewEnded() {
  isPreviewPlaying.value = false;
}

function resetSilenceSettings() {
  silence.value = { ...DEFAULT_SILENCE };
}

function handleWindowKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && activeOverlay.value) {
    closeOverlay();
  }
}

watch(splitMode, async (mode) => {
  if (mode === 'manual' && batchFiles.value.length === 1 && !mediaProbe.value) {
    await refreshSelectedFile(batchFiles.value[0]);
  }
});

watch(downloadAudioTarget, (target) => {
  if (target === 'azuracast') {
    format.value = 'mp3';
    quality.value = '320';
  }
});

watch(batchAudioTarget, (target) => {
  if (target === 'azuracast') {
    batchFormat.value = 'mp3';
    normalizeAudio.value = true;
  }
});

watch(loudnessTargetLufs, (target) => {
  const clamped = clamp(target, -30, -6);
  if (target !== clamped) {
    loudnessTargetLufs.value = clamped;
  }
});

watch(studioMode, (nextMode, previousMode) => {
  persistSettings(previousMode);
  restoreSettings(nextMode);
  batchFiles.value = [];
  clearSingleFilePreview();
  addLog(`Switched to ${nextMode === 'audio' ? 'Audio' : 'Video'} Studio`, 'info');
});

watch(url, () => {
  if (downloadStep.value !== 'url') {
    resetFormatExplorer();
  }
});

watch(format, (nextFormat) => {
  if (!activeDownloadFormats.value.includes(nextFormat)) {
    format.value = activeDownloadFormats.value[0] ?? 'mp3';
    return;
  }

  if (!isAudioStudio.value && downloadAudioTarget.value === 'azuracast') {
    downloadAudioTarget.value = 'general';
  }

  if (isAudioStudio.value && downloadVideoTarget.value !== 'general') {
    downloadVideoTarget.value = 'general';
  }

  const validQualities = isAudioStudio.value
    ? ['best', ...AUDIO_BITRATES]
    : ['best', ...VIDEO_QUALITIES];
  if (!validQualities.includes(quality.value)) {
    quality.value = 'best';
  }
});

watch(downloadVideoTarget, (target) => {
  if (target === 'google_tv_cast') {
    format.value = 'mp4';
    quality.value = '2160';
  }
});

watch(batchFormat, (nextFormat) => {
  if (!activeBatchFormats.value.includes(nextFormat)) {
    batchFormat.value = activeBatchFormats.value[0] ?? 'mp3';
    return;
  }

  if (!isAudioStudio.value && batchAudioTarget.value === 'azuracast') {
    batchAudioTarget.value = 'general';
  }

  if (!AUDIO_FORMATS.includes(nextFormat) && activeOverlay.value === 'processAudio') {
    closeOverlay();
  }
});

watch(currentDuration, (duration) => {
  timelineCursor.value = clamp(timelineCursor.value, 0, duration);
  manualMarkers.value = manualMarkers.value.filter(
    (marker) => marker > 0.05 && marker < duration - 0.05,
  );

  if (duration > 0) {
    const current = normalizeTrimBounds(
      duration,
      parseTimeInput(trimStart.value) ?? 0,
      parseTimeInput(trimEnd.value) ?? duration,
    );
    syncTrimInputs(current.start, current.end);
  }
});

watch(activeTab, () => {
  closeOverlay();
});

watch(
  queue,
  () => {
    scheduleQueuePersistence();
  },
  { deep: true },
);

watch(
  [
    format,
    quality,
    playlistMode,
    downloadAudioTarget,
    downloadVideoTarget,
    downloadDestinationMode,
    downloadPath,
    batchDestinationMode,
    batchOutputDir,
    batchFormat,
    normalizeAudio,
    loudnessTargetLufs,
    batchAudioTarget,
    eq,
    fadeInDuration,
    fadeOutDuration,
    downloadPipelineSplitMode,
    downloadPipelineSilence,
    studioMode,
  ],
  () => {
    persistSettings();
  },
  { deep: true },
);

onMounted(async () => {
  try {
    systemDownloadDir.value = await downloadDir();
  } catch {
    systemDownloadDir.value = 'Downloads';
  }

  restoreActiveStudioMode();
  restoreSettings();
  restoreQueueHistory();
  await restoreSavedCookiesFile();

  const unlisten = await listen<JobProgressPayload>('job-progress', (event) => {
    updateQueueItem(event.payload);
  });

  unlisteners.value.push(unlisten);
  window.addEventListener('keydown', handleWindowKeydown);
});

onUnmounted(() => {
  stopPointerTracking?.();
  window.removeEventListener('keydown', handleWindowKeydown);
  if (queuePersistenceTimer !== null) {
    window.clearTimeout(queuePersistenceTimer);
  }

  for (const unlisten of unlisteners.value) {
    unlisten();
  }
});
</script>

<template>
  <div class="h-full overflow-y-auto">
    <div class="p-6 md:p-8 max-w-[1600px] mx-auto space-y-5 h-full flex flex-col min-h-0">
      <section class="sticky top-0 z-20 rounded-[28px] border border-white/10 bg-[#08111f]/90 backdrop-blur-2xl shadow-[0_20px_80px_rgba(0,0,0,0.35)] px-4 py-4">
        <div class="flex flex-col xl:flex-row xl:items-center xl:justify-between gap-4">
          <div class="flex items-center gap-3">
            <div class="h-12 w-12 rounded-2xl bg-cyan-500/10 ring-1 ring-cyan-400/25 flex items-center justify-center text-cyan-300">
              <Scissors class="w-5 h-5" />
            </div>
            <div>
              <p class="text-[11px] uppercase tracking-[0.28em] text-cyan-400/80 font-semibold">Studio</p>
              <h1 class="text-lg md:text-xl font-semibold text-slate-100">
                {{ isAudioStudio ? 'Audio Studio' : 'Video Studio' }}
              </h1>
            </div>
          </div>

          <div class="flex rounded-2xl border border-white/10 bg-white/[0.03] p-1">
            <button
              @click="studioMode = 'audio'"
              class="px-4 py-2 rounded-xl text-sm font-semibold transition-colors"
              :class="isAudioStudio ? 'bg-cyan-400 text-slate-950' : 'text-slate-300 hover:bg-white/[0.05]'"
            >
              Audio
            </button>
            <button
              @click="studioMode = 'video'"
              class="px-4 py-2 rounded-xl text-sm font-semibold transition-colors"
              :class="!isAudioStudio ? 'bg-cyan-400 text-slate-950' : 'text-slate-300 hover:bg-white/[0.05]'"
            >
              Video
            </button>
          </div>

          <div class="flex flex-wrap gap-2">
            <button
              v-for="tab in TABS"
              :key="tab.value"
              @click="activeTab = tab.value"
              class="min-w-[150px] rounded-2xl px-4 py-3 border text-left transition-all duration-200"
              :class="activeTab === tab.value
                ? 'border-cyan-400/40 bg-cyan-400/10 text-white shadow-[0_0_0_1px_rgba(34,211,238,0.08)]'
                : 'border-white/10 bg-white/[0.02] text-slate-300 hover:border-cyan-400/25 hover:bg-cyan-400/5'"
            >
              <div class="text-sm font-medium">{{ tab.label }}</div>
              <div class="text-[11px] text-slate-400 mt-1">{{ tab.hint }}</div>
            </button>
          </div>

          <div class="flex gap-2 text-xs text-slate-400">
            <span class="px-3 py-2 rounded-full bg-white/[0.04] border border-white/10">
              {{ queueStats.total }} total
            </span>
            <span class="px-3 py-2 rounded-full bg-white/[0.04] border border-white/10">
              {{ queueStats.active }} active
            </span>
            <span class="px-3 py-2 rounded-full bg-white/[0.04] border border-white/10">
              {{ queueStats.waiting }} waiting
            </span>
          </div>
        </div>
      </section>

      <template v-if="activeTab === 'downloads'">
        <section class="grid grid-cols-1 xl:grid-cols-[minmax(0,1.2fr)_360px] gap-5 xl:flex-1 xl:min-h-0">
          <div class="rounded-[28px] border border-white/10 bg-[#091626] shadow-[0_20px_60px_rgba(0,0,0,0.22)] p-6 md:p-7 space-y-5 xl:h-full xl:min-h-0 xl:overflow-y-auto">
            <!-- Step 1: URL -->
            <template v-if="downloadStep === 'url'">
              <div class="space-y-2">
                <p class="text-xs uppercase tracking-[0.24em] text-cyan-400/80 font-semibold">Step 1 of 3</p>
                <h2 class="text-2xl font-semibold text-slate-50">Paste a URL</h2>
                <p class="text-sm text-slate-400 max-w-2xl">Enter a video or playlist URL to discover available formats before downloading.</p>
              </div>

              <div class="relative group">
                <div class="absolute left-4 top-1/2 -translate-y-1/2 text-slate-500 pointer-events-none group-focus-within:text-cyan-400">
                  <Youtube class="w-5 h-5" />
                </div>
                <input
                  v-model="url"
                  @keyup.enter="exploreFormats"
                  type="text"
                  placeholder="Paste a video or playlist URL"
                  class="w-full h-14 rounded-2xl border border-white/10 bg-[#050c18] pl-12 pr-4 text-slate-100 placeholder:text-slate-500 focus:outline-none focus:ring-1 focus:ring-cyan-400/50 focus:border-cyan-400/40"
                />
              </div>

              <div v-if="formatListError" class="rounded-2xl border border-rose-400/20 bg-rose-400/[0.06] px-4 py-3 text-sm text-rose-100">
                {{ formatListError }}
              </div>

              <div class="overflow-hidden rounded-[22px] border border-white/10 bg-[#060d18]">
                <table class="w-full table-fixed border-collapse">
                  <tbody class="divide-y divide-white/10">
                    <tr v-for="row in downloadSummaryRows" :key="row.label" class="align-top">
                      <th class="w-[160px] px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">{{ row.label }}</th>
                      <td class="px-4 py-3">
                        <div class="text-sm font-medium text-slate-100 break-words">{{ row.value }}</div>
                        <div v-if="row.detail" class="mt-1 text-xs text-slate-400">{{ row.detail }}</div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>

            <!-- Step 2: Pick format -->
            <template v-else-if="downloadStep === 'formats'">
              <div class="space-y-2">
                <div class="flex items-center gap-3">
                  <p class="text-xs uppercase tracking-[0.24em] text-cyan-400/80 font-semibold">Step 2 of 3</p>
                  <button @click="resetFormatExplorer" class="text-xs text-cyan-300 hover:text-cyan-200 transition-colors">Change URL</button>
                </div>
                <h2 class="text-2xl font-semibold text-slate-50">Pick a format</h2>
                <p v-if="formatListTitle" class="text-sm text-slate-300">{{ formatListTitle }}</p>
              </div>

              <div v-if="!formatList.length" class="rounded-2xl border border-dashed border-white/10 bg-white/[0.02] px-4 py-8 text-sm text-slate-400 text-center">
                No formats returned for this URL. Try a different link or check your cookies.
              </div>

              <div class="space-y-3">
                <!-- Combined formats -->
                <div v-if="formatList.filter((f) => f.hasVideo && f.hasAudio).length">
                  <p class="text-xs uppercase tracking-[0.22em] text-slate-400 mb-2">Video + Audio</p>
                  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                    <button
                      v-for="fmt in formatList.filter((f) => f.hasVideo && f.hasAudio)"
                      :key="fmt.formatId"
                      @click="selectedFormatId = fmt.formatId"
                      class="rounded-2xl border px-4 py-3 text-left transition-colors"
                      :class="selectedFormatId === fmt.formatId
                        ? 'border-cyan-400/40 bg-cyan-400/[0.10] text-white'
                        : 'border-white/10 bg-[#060d18] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">{{ fmt.resolution || fmt.formatNote || fmt.formatId }}</div>
                      <div class="text-[11px] text-slate-400 mt-1">
                        {{ fmt.ext.toUpperCase() }}
                        <span v-if="fmt.fps"> • {{ fmt.fps }}fps</span>
                        <span v-if="fmt.vcodec"> • {{ fmt.vcodec.split('.')[0] }}</span>
                        <span v-if="fmt.acodec"> • {{ fmt.acodec.split('.')[0] }}</span>
                        <span v-if="fmt.filesize || fmt.filesizeApprox"> • {{ formatBytes(fmt.filesize || fmt.filesizeApprox || 0) }}</span>
                      </div>
                      <div class="text-[10px] text-slate-500 mt-0.5">ID: {{ fmt.formatId }}</div>
                    </button>
                  </div>
                </div>

                <!-- Audio only -->
                <div v-if="formatList.filter((f) => !f.hasVideo && f.hasAudio).length">
                  <p class="text-xs uppercase tracking-[0.22em] text-slate-400 mb-2">Audio only</p>
                  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                    <button
                      v-for="fmt in formatList.filter((f) => !f.hasVideo && f.hasAudio)"
                      :key="fmt.formatId"
                      @click="selectedFormatId = fmt.formatId"
                      class="rounded-2xl border px-4 py-3 text-left transition-colors"
                      :class="selectedFormatId === fmt.formatId
                        ? 'border-cyan-400/40 bg-cyan-400/[0.10] text-white'
                        : 'border-white/10 bg-[#060d18] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">{{ fmt.formatNote || fmt.formatId }}</div>
                      <div class="text-[11px] text-slate-400 mt-1">
                        {{ fmt.ext.toUpperCase() }}
                        <span v-if="fmt.acodec"> • {{ fmt.acodec.split('.')[0] }}</span>
                        <span v-if="fmt.filesize || fmt.filesizeApprox"> • {{ formatBytes(fmt.filesize || fmt.filesizeApprox || 0) }}</span>
                      </div>
                      <div class="text-[10px] text-slate-500 mt-0.5">ID: {{ fmt.formatId }}</div>
                    </button>
                  </div>
                </div>

                <!-- Video only -->
                <div v-if="formatList.filter((f) => f.hasVideo && !f.hasAudio).length">
                  <p class="text-xs uppercase tracking-[0.22em] text-slate-400 mb-2">Video only (no audio)</p>
                  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                    <button
                      v-for="fmt in formatList.filter((f) => f.hasVideo && !f.hasAudio)"
                      :key="fmt.formatId"
                      @click="selectedFormatId = fmt.formatId"
                      class="rounded-2xl border px-4 py-3 text-left transition-colors"
                      :class="selectedFormatId === fmt.formatId
                        ? 'border-cyan-400/40 bg-cyan-400/[0.10] text-white'
                        : 'border-white/10 bg-[#060d18] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">{{ fmt.resolution || fmt.formatNote || fmt.formatId }}</div>
                      <div class="text-[11px] text-slate-400 mt-1">
                        {{ fmt.ext.toUpperCase() }}
                        <span v-if="fmt.fps"> • {{ fmt.fps }}fps</span>
                        <span v-if="fmt.vcodec"> • {{ fmt.vcodec.split('.')[0] }}</span>
                        <span v-if="fmt.filesize || fmt.filesizeApprox"> • {{ formatBytes(fmt.filesize || fmt.filesizeApprox || 0) }}</span>
                      </div>
                      <div class="text-[10px] text-slate-500 mt-0.5">ID: {{ fmt.formatId }}</div>
                    </button>
                  </div>
                </div>
              </div>
            </template>

            <!-- Step 3: Confirm -->
            <template v-else-if="downloadStep === 'confirm'">
              <div class="space-y-2">
                <div class="flex items-center gap-3">
                  <p class="text-xs uppercase tracking-[0.24em] text-cyan-400/80 font-semibold">Step 3 of 3</p>
                  <button @click="downloadStep = 'formats'" class="text-xs text-cyan-300 hover:text-cyan-200 transition-colors">Change format</button>
                </div>
                <h2 class="text-2xl font-semibold text-slate-50">Confirm download</h2>
                <p v-if="formatListTitle" class="text-sm text-slate-300">{{ formatListTitle }}</p>
              </div>

              <div v-if="selectedFormat" class="rounded-[22px] border border-cyan-400/20 bg-cyan-400/[0.06] p-4 space-y-2">
                <p class="text-xs uppercase tracking-[0.18em] text-cyan-200/80">Selected format</p>
                <div class="text-sm font-medium text-slate-100">{{ selectedFormat.resolution || selectedFormat.formatNote || selectedFormat.formatId }}</div>
                <div class="text-xs text-slate-400">
                  {{ selectedFormat.ext.toUpperCase() }}
                  <span v-if="selectedFormat.vcodec"> • Video: {{ selectedFormat.vcodec }}</span>
                  <span v-if="selectedFormat.acodec"> • Audio: {{ selectedFormat.acodec }}</span>
                  <span v-if="selectedFormat.filesize || selectedFormat.filesizeApprox"> • Size: {{ formatBytes(selectedFormat.filesize || selectedFormat.filesizeApprox || 0) }}</span>
                </div>
                <div class="text-[11px] text-slate-500">Format ID: {{ selectedFormat.formatId }}</div>
              </div>

              <div class="overflow-hidden rounded-[22px] border border-white/10 bg-[#060d18]">
                <table class="w-full table-fixed border-collapse">
                  <tbody class="divide-y divide-white/10">
                    <tr v-for="row in downloadSummaryRows" :key="row.label" class="align-top">
                      <th class="w-[160px] px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">{{ row.label }}</th>
                      <td class="px-4 py-3">
                        <div class="text-sm font-medium text-slate-100 break-words">{{ row.value }}</div>
                        <div v-if="row.detail" class="mt-1 text-xs text-slate-400">{{ row.detail }}</div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>
          </div>

          <aside class="rounded-[28px] border border-white/10 bg-[#0b1625] p-5 space-y-4 xl:h-full xl:min-h-0 overflow-y-auto">
            <div class="rounded-[22px] border border-white/10 bg-[#060d18] p-4 space-y-3">
              <p class="text-[11px] uppercase tracking-[0.22em] text-slate-400">Quick Controls</p>
              <div class="grid grid-cols-1 gap-2">
                <button
                  @click="openOverlay('downloadOptions')"
                  class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/25 transition-colors"
                >
                  Download options
                </button>
                <button
                  @click="openOverlay('downloadAccess')"
                  class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/25 transition-colors"
                >
                  Cookies file
                </button>
              </div>
            </div>

            <div class="rounded-[22px] border border-cyan-400/15 bg-cyan-400/[0.04] p-4 space-y-3">
              <p class="text-[11px] uppercase tracking-[0.22em] text-cyan-200/80">Queue Actions</p>

              <template v-if="downloadStep === 'url'">
                <button
                  @click="exploreFormats"
                  :disabled="!url.trim() || formatListLoading"
                  class="w-full inline-flex items-center justify-center whitespace-nowrap h-11 px-5 rounded-2xl bg-cyan-400 text-slate-950 font-semibold hover:bg-cyan-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  <template v-if="formatListLoading">Exploring formats...</template>
                  <template v-else>Explore formats</template>
                </button>
              </template>

              <template v-else-if="downloadStep === 'formats'">
                <button
                  @click="downloadStep = 'confirm'"
                  :disabled="!selectedFormatId"
                  class="w-full inline-flex items-center justify-center whitespace-nowrap h-11 px-5 rounded-2xl bg-cyan-400 text-slate-950 font-semibold hover:bg-cyan-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Continue to confirm
                </button>
              </template>

              <template v-else-if="downloadStep === 'confirm'">
                <button
                  @click="addDownloadToQueue"
                  :disabled="!canConfirmDownload"
                  class="w-full inline-flex items-center justify-center whitespace-nowrap h-11 px-5 rounded-2xl bg-cyan-400 text-slate-950 font-semibold hover:bg-cyan-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Add to queue
                </button>
              </template>
            </div>
          </aside>
        </section>
      </template>

      <template v-else-if="activeTab === 'process'">
        <section class="grid grid-cols-1 xl:grid-cols-[minmax(0,1.2fr)_360px] gap-5 xl:flex-1 xl:min-h-0">
          <section class="rounded-[28px] border border-white/10 bg-[#091626] p-6 md:p-7 space-y-5 xl:h-full xl:min-h-0 xl:overflow-y-auto">
            <div class="flex flex-col md:flex-row md:items-end md:justify-between gap-4">
              <div>
                <p class="text-xs uppercase tracking-[0.22em] text-cyan-400/80 font-semibold">Processing</p>
                <h2 class="text-2xl font-semibold text-slate-50 mt-2">Prepare the {{ currentProcessModeLabel }} chain</h2>
                <p class="text-sm text-slate-400 mt-2 max-w-2xl">
                  Keep this stage focused on source selection and one compact processing table. Advanced controls stay tucked into inspector panels.
                </p>
              </div>
              <button
                @click="selectBatchFiles"
                class="inline-flex items-center justify-center whitespace-nowrap h-11 px-4 rounded-2xl border border-cyan-400/25 bg-cyan-400/[0.08] text-cyan-200 hover:bg-cyan-400/[0.12] transition-colors"
              >
                Choose Files
              </button>
            </div>

            <div v-if="batchFiles.length" class="flex flex-wrap gap-2">
              <span
                v-for="filePath in batchFiles.slice(0, 10)"
                :key="filePath"
                class="px-3 py-1 rounded-full border border-white/10 bg-black/20 text-xs text-slate-300 max-w-full truncate"
              >
                {{ basename(filePath) }}
              </span>
              <span
                v-if="batchFiles.length > 10"
                class="px-3 py-1 rounded-full border border-white/10 bg-black/20 text-xs text-slate-300"
              >
                +{{ batchFiles.length - 10 }} more
              </span>
            </div>

            <div class="overflow-hidden rounded-[22px] border border-white/10 bg-[#060d18]">
              <table class="w-full table-fixed border-collapse">
                <tbody class="divide-y divide-white/10">
                  <tr v-for="row in processSummaryRows" :key="row.label" class="align-top">
                    <th class="w-[160px] px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">
                      {{ row.label }}
                    </th>
                    <td class="px-4 py-3">
                      <div class="text-sm font-medium text-slate-100 break-words">{{ row.value }}</div>
                      <div v-if="row.detail" class="mt-1 text-xs text-slate-400">{{ row.detail }}</div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>

          <aside class="rounded-[28px] border border-white/10 bg-[#0b1625] p-5 space-y-4 xl:h-full xl:min-h-0 overflow-y-auto">
            <div class="rounded-[22px] border border-white/10 bg-[#060d18] p-4 space-y-3">
              <p class="text-[11px] uppercase tracking-[0.22em] text-slate-400">Current Chain</p>
              <p class="text-sm text-slate-100">{{ processSummary }}</p>
              <p class="text-xs text-slate-400 leading-6">
                This chain is reused both for local batch processing and for `Download & Process` jobs in the current studio.
              </p>
            </div>

            <div class="rounded-[22px] border border-white/10 bg-[#060d18] p-4 space-y-3">
              <p class="text-[11px] uppercase tracking-[0.22em] text-slate-400">Inspector</p>
              <div class="grid grid-cols-1 gap-2">
                <button
                  @click="openOverlay('processOutput')"
                  class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors gap-2"
                >
                  <Settings2 class="w-4 h-4" />
                  Output settings
                </button>
                <button
                  v-if="isAudioBatch"
                  @click="openOverlay('processAudio')"
                  class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors gap-2"
                >
                  <SlidersHorizontal class="w-4 h-4" />
                  Audio chain
                </button>
                <button
                  @click="activeTab = 'split'"
                  class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors gap-2"
                >
                  <Scissors class="w-4 h-4" />
                  Split / Trim
                </button>
              </div>
            </div>

            <div class="rounded-[22px] border border-cyan-400/15 bg-cyan-400/[0.04] p-4 space-y-3">
              <p class="text-[11px] uppercase tracking-[0.22em] text-cyan-200/80">Queue Actions</p>
              <button
                @click="addBatchToQueue"
                :disabled="!canQueueBatch"
                class="w-full inline-flex items-center justify-center whitespace-nowrap h-11 px-5 rounded-2xl bg-cyan-400 text-slate-950 font-semibold hover:bg-cyan-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Add Processing Jobs
              </button>
            </div>
          </aside>
        </section>
      </template>

      <template v-else-if="activeTab === 'split'">
        <section class="space-y-5">
          <section class="rounded-[32px] border border-white/10 bg-[#091626] p-6 md:p-7 space-y-6">
            <div class="flex flex-col xl:flex-row xl:items-end xl:justify-between gap-4">
              <div>
                <p class="text-xs uppercase tracking-[0.22em] text-cyan-400/80 font-semibold">Timeline</p>
                <h2 class="text-3xl font-semibold text-slate-50 mt-2">Trim and split visually</h2>
                <p class="text-sm text-slate-400 mt-2 max-w-2xl">
                  Edit on the waveform, preview playback in place, and keep detailed controls in panels instead of separate shortcut cards.
                </p>
              </div>
              <button
                @click="selectBatchFiles"
                class="inline-flex items-center justify-center whitespace-nowrap h-11 px-4 rounded-2xl border border-cyan-400/20 bg-cyan-400/[0.06] text-cyan-200 hover:bg-cyan-400/[0.10] transition-colors"
              >
                Choose Files
              </button>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-3 gap-3">
              <button
                v-for="mode in SPLIT_MODES"
                :key="mode.value"
                @click="splitMode = mode.value"
                class="rounded-[22px] border px-5 py-4 text-left transition-colors"
                :class="splitMode === mode.value
                  ? 'border-cyan-400/30 bg-cyan-400/[0.07] text-white'
                  : 'border-white/10 bg-[#0a1422] text-slate-300 hover:border-cyan-400/16'"
              >
                <div class="text-base font-medium">{{ mode.label }}</div>
                <div class="text-xs text-slate-400 mt-1 leading-5">{{ mode.hint }}</div>
              </button>
            </div>

            <div
              v-if="isAutoSplitMode"
              class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-5"
            >
              <div class="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
                <div>
                  <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Auto split settings</p>
                  <p class="text-sm text-slate-300 mt-2">These controls apply only to the current split operation and are reset on app restart.</p>
                </div>
                <button
                  @click="resetSilenceSettings"
                  class="h-10 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                >
                  Reset defaults
                </button>
              </div>

              <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                <label class="space-y-2">
                  <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Threshold (dB)</span>
                  <input
                    v-model.number="silence.thresholdDb"
                    type="number"
                    step="1"
                    class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                  />
                </label>

                <label class="space-y-2">
                  <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Minimum silence</span>
                  <input
                    v-model.number="silence.minSilenceDuration"
                    type="number"
                    min="0.1"
                    step="0.1"
                    class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                  />
                </label>

                <label class="space-y-2">
                  <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Minimum segment</span>
                  <input
                    v-model.number="silence.minSegmentDuration"
                    type="number"
                    min="1"
                    step="1"
                    class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                  />
                </label>
              </div>
            </div>

            <div
              v-if="isChapterSplitMode"
              class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-3"
            >
              <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Source chapter split</p>
              <p class="text-sm text-slate-300 leading-6">
                MediaToolsPro will read embedded chapter marks from the selected file and cut at those timestamps.
                If the source has no valid chapters, the same silence settings below are used as a fallback.
              </p>
              <div class="grid grid-cols-1 md:grid-cols-3 gap-4 pt-2">
                <label class="space-y-2">
                  <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Fallback threshold (dB)</span>
                  <input
                    v-model.number="silence.thresholdDb"
                    type="number"
                    step="1"
                    class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                  />
                </label>

                <label class="space-y-2">
                  <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Fallback silence</span>
                  <input
                    v-model.number="silence.minSilenceDuration"
                    type="number"
                    min="0.1"
                    step="0.1"
                    class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                  />
                </label>

                <label class="space-y-2">
                  <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Fallback min segment</span>
                  <input
                    v-model.number="silence.minSegmentDuration"
                    type="number"
                    min="1"
                    step="1"
                    class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                  />
                </label>
              </div>
            </div>

            <div
              v-if="isManualSplitMode"
              class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-5"
            >
              <div>
                <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Manual marks</p>
                <p class="text-sm text-slate-300 mt-2">These marks are per-operation and are not restored after restarting the app.</p>
              </div>

              <div class="flex flex-col sm:flex-row gap-3">
                <input
                  v-model="manualMarkerInput"
                  @keyup.enter="addMarkerFromInput"
                  type="text"
                  placeholder="e.g. 02:35.5"
                  class="flex-1 h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 placeholder:text-slate-500 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                />
                <button
                  @click="addMarkerFromInput"
                  class="h-12 px-5 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                >
                  Add Mark
                </button>
                <button
                  @click="addMarkerAtCursor"
                  class="h-12 px-5 rounded-2xl border border-cyan-400/20 bg-cyan-400/[0.07] text-cyan-100 hover:bg-cyan-400/[0.12] transition-colors"
                >
                  Add at cursor
                </button>
              </div>

              <div v-if="manualMarkers.length" class="flex flex-wrap gap-2">
                <button
                  v-for="marker in manualMarkers"
                  :key="marker"
                  @click="removeMarker(marker)"
                  class="px-3 py-1 rounded-full border border-amber-300/20 bg-amber-300/[0.08] text-xs text-amber-100 hover:bg-amber-300/[0.14] transition-colors"
                >
                  {{ formatDisplayTime(marker) }} x
                </button>
              </div>

              <div v-else class="rounded-[22px] border border-dashed border-white/10 bg-white/[0.02] px-4 py-6 text-sm text-slate-400">
                No manual marks yet. Add one from the cursor or type an exact time.
              </div>
            </div>

            <div v-if="!hasBatchFiles" class="rounded-[26px] border border-dashed border-white/10 bg-[#0a1422] px-6 py-16 text-center text-slate-400">
              Choose a source file to load the visual timeline.
            </div>

            <div
              v-else-if="!hasSingleBatchFile"
              class="rounded-[26px] border border-amber-400/20 bg-amber-400/[0.06] px-6 py-10 text-sm text-amber-100"
            >
              The visual timeline is shown for one source file at a time. You can still queue shared trim or silence settings for the full batch.
            </div>

            <template v-else>
              <div class="rounded-[30px] border border-white/10 bg-[#060d18] overflow-hidden">
                <div class="flex flex-col xl:flex-row xl:items-center xl:justify-between gap-4 px-6 py-5 border-b border-white/10">
                  <div class="min-w-0">
                    <p class="text-xl font-semibold text-slate-50 truncate">
                      {{ mediaProbe?.title || basename(selectedBatchFile) }}
                    </p>
                    <p class="text-sm text-slate-400 mt-1">
                      {{ mediaProbe?.durationLabel || waveformPreview?.durationLabel || 'Inspecting duration...' }}
                    </p>
                  </div>

                  <div class="flex flex-wrap gap-2">
                    <span class="px-3 py-2 rounded-full border border-white/10 bg-white/[0.04] text-xs text-slate-300">
                      Cursor {{ timelineCursorLabel }}
                    </span>
                    <span class="px-3 py-2 rounded-full border border-white/10 bg-white/[0.04] text-xs text-slate-300">
                      Start {{ trimStart || '00:00' }}
                    </span>
                    <span class="px-3 py-2 rounded-full border border-white/10 bg-white/[0.04] text-xs text-slate-300">
                      End {{ trimEnd || 'end' }}
                    </span>
                  </div>
                </div>

                <div class="px-6 py-6">
                  <audio
                    ref="audioPreviewElement"
                    :src="audioPreviewUrl"
                    preload="metadata"
                    class="hidden"
                    @timeupdate="onPreviewTimeUpdate"
                    @play="onPreviewPlay"
                    @pause="onPreviewPause"
                    @ended="onPreviewEnded"
                  />

                  <div class="rounded-[24px] border border-white/10 bg-[#09111d] px-4 py-4 mb-5">
                    <div class="flex flex-col gap-4">
                      <div class="flex flex-col xl:flex-row xl:items-center gap-4">
                        <div class="flex items-center gap-2">
                          <button
                            @click="togglePreviewPlayback"
                            class="h-11 w-11 rounded-full bg-cyan-400 text-slate-950 flex items-center justify-center hover:bg-cyan-300 transition-colors"
                            :disabled="!audioPreviewUrl"
                          >
                            <Pause v-if="isPreviewPlaying" class="w-4 h-4" />
                            <Play v-else class="w-4 h-4 ml-0.5" />
                          </button>

                          <button
                            @click="seekPreview(-5)"
                            class="h-11 w-11 rounded-full border border-white/10 bg-[#060d18] text-slate-200 flex items-center justify-center hover:border-cyan-400/20 transition-colors"
                            :disabled="!audioPreviewUrl"
                          >
                            <RotateCcw class="w-4 h-4" />
                          </button>

                          <button
                            @click="seekPreview(5)"
                            class="h-11 w-11 rounded-full border border-white/10 bg-[#060d18] text-slate-200 flex items-center justify-center hover:border-cyan-400/20 transition-colors"
                            :disabled="!audioPreviewUrl"
                          >
                            <RotateCw class="w-4 h-4" />
                          </button>
                        </div>

                        <div class="flex-1 min-w-0 space-y-2">
                          <div class="flex items-center justify-between text-xs text-slate-400">
                            <span>Preview while editing</span>
                            <span>{{ formatDisplayTime(trimSelection.start) }} -> {{ formatDisplayTime(trimSelection.end) }}</span>
                          </div>
                          <input
                            type="range"
                            min="0"
                            :max="Math.max(currentDuration, 0)"
                            step="0.05"
                            :value="timelineCursor"
                            @input="handleCursorSlider"
                            class="w-full accent-cyan-400"
                          />
                        </div>

                        <button
                          @click="loopSelection = !loopSelection"
                          class="inline-flex items-center justify-center whitespace-nowrap h-11 px-4 rounded-2xl border gap-2 transition-colors"
                          :class="loopSelection
                            ? 'border-cyan-400/30 bg-cyan-400/[0.08] text-cyan-200'
                            : 'border-white/10 bg-[#060d18] text-slate-300 hover:border-cyan-400/20'"
                        >
                          <Repeat2 class="w-4 h-4" />
                          Loop selection
                        </button>
                      </div>

                      <div class="flex flex-wrap gap-3">
                        <button
                          @click="setTrimFromCursor('start')"
                          class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                        >
                          Use cursor as start
                        </button>
                        <button
                          @click="setTrimFromCursor('end')"
                          class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                        >
                          Use cursor as end
                        </button>
                        <button
                          v-if="isManualSplitMode"
                          @click="addMarkerAtCursor"
                          class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                        >
                          Add mark at cursor
                        </button>
                        <button
                          @click="resetTrimSelection"
                          class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                        >
                          Reset trim
                        </button>
                      </div>
                    </div>
                  </div>

                  <div
                    v-if="waveformLoading"
                    class="h-[340px] rounded-[26px] border border-white/10 bg-[#020812] animate-pulse"
                  ></div>

                  <div
                    v-else
                    ref="waveformTrack"
                    class="relative h-[340px] rounded-[26px] border border-white/10 overflow-hidden cursor-crosshair bg-[#020812]"
                    @click="handleWaveformClick"
                  >
                    <img
                      v-if="waveformImageUrl"
                      :src="waveformImageUrl"
                      alt="Waveform preview"
                      class="absolute inset-0 w-full h-full object-cover pointer-events-none opacity-95"
                    />
                    <div
                      v-else
                      class="absolute inset-0 bg-[radial-gradient(circle_at_top_left,rgba(34,211,238,0.14),transparent_34%),linear-gradient(180deg,rgba(255,255,255,0.02),rgba(255,255,255,0.01))]"
                    ></div>

                    <div
                      class="absolute inset-y-0 left-0 bg-black/35 pointer-events-none"
                      :style="{ width: `${trimStartPercent}%` }"
                    ></div>
                    <div
                      class="absolute inset-y-0 border-x-2 border-cyan-400 bg-cyan-400/[0.10] pointer-events-none"
                      :style="{ left: `${trimStartPercent}%`, width: `${trimSelectionWidth}%` }"
                    ></div>
                    <div
                      class="absolute inset-y-0 right-0 bg-black/35 pointer-events-none"
                      :style="{ width: `${100 - trimEndPercent}%` }"
                    ></div>

                    <div
                      v-for="marker in manualMarkers"
                      :key="marker"
                      class="absolute top-0 bottom-0 w-[2px] bg-amber-300/90 pointer-events-none"
                      :style="{ left: `${markerPosition(marker)}%` }"
                    ></div>

                    <div
                      class="absolute top-0 bottom-0 w-[2px] bg-cyan-300 pointer-events-none"
                      :style="{ left: `${timelineCursorPercent}%` }"
                    ></div>

                    <button
                      class="absolute top-5 bottom-5 w-4 -ml-2 rounded-full bg-cyan-400 border border-white/80 shadow-[0_8px_30px_rgba(34,211,238,0.35)] cursor-ew-resize"
                      :style="{ left: `${trimStartPercent}%` }"
                      @pointerdown.stop="beginTrimDrag('start', $event)"
                    ></button>
                    <button
                      class="absolute top-5 bottom-5 w-4 -ml-2 rounded-full bg-cyan-400 border border-white/80 shadow-[0_8px_30px_rgba(34,211,238,0.35)] cursor-ew-resize"
                      :style="{ left: `${trimEndPercent}%` }"
                      @pointerdown.stop="beginTrimDrag('end', $event)"
                    ></button>

                    <div class="absolute inset-x-0 bottom-4 px-5 flex justify-between text-xs text-slate-300 pointer-events-none">
                      <span>00:00</span>
                      <span>{{ mediaProbe?.durationLabel || waveformPreview?.durationLabel || '--:--' }}</span>
                    </div>
                  </div>

                  <p class="text-xs text-slate-400 mt-3">
                    Click the waveform to move the cursor, drag the cyan handles to trim, and preview playback without leaving the editor.
                  </p>
                </div>
              </div>

              <div
                v-if="waveformError"
                class="rounded-2xl border border-amber-400/20 bg-amber-400/[0.06] px-4 py-3 text-sm text-amber-100"
              >
                The waveform preview could not be generated for this file, but trim and split settings still work.
              </div>

              <div class="overflow-hidden rounded-[22px] border border-white/10 bg-[#060d18]">
                <table class="w-full table-fixed border-collapse">
                  <tbody class="divide-y divide-white/10">
                    <tr v-for="row in splitSummaryRows" :key="row.label" class="align-top">
                      <th class="w-[180px] px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">
                        {{ row.label }}
                      </th>
                      <td class="px-4 py-3">
                        <div class="text-sm font-medium text-slate-100 break-words">{{ row.value }}</div>
                        <div v-if="row.detail" class="mt-1 text-xs text-slate-400">{{ row.detail }}</div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <div
                v-if="manualSplitBlocked"
                class="rounded-[20px] border border-amber-400/20 bg-amber-400/[0.06] px-4 py-3 text-sm text-amber-100"
              >
                Manual split needs exactly one selected source file so the timeline matches the real duration.
              </div>

              <div class="flex flex-col xl:flex-row xl:items-center xl:justify-between gap-3 rounded-[22px] border border-cyan-400/15 bg-cyan-400/[0.04] px-4 py-4">
                <div>
                  <p class="text-sm font-medium text-slate-100">Editing actions</p>
                  <p class="text-xs text-slate-400 mt-1">Split-specific controls stay visible inline so each operation is easy to review before queueing.</p>
                </div>
                <div class="flex flex-wrap gap-3">
                  <button
                    @click="openOverlay('trimTools')"
                    class="inline-flex items-center justify-center whitespace-nowrap h-11 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/20 transition-colors gap-2"
                  >
                    <Scissors class="w-4 h-4" />
                    Trim tools
                  </button>
                  <button
                    @click="addBatchToQueue"
                    :disabled="!canQueueBatch"
                    class="inline-flex items-center justify-center whitespace-nowrap h-11 px-5 rounded-2xl bg-cyan-400 text-slate-950 font-semibold hover:bg-cyan-300 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    Add Split / Trim Jobs
                  </button>
                </div>
              </div>
            </template>
          </section>
        </section>
      </template>

      <template v-else-if="activeTab === 'queue'">
        <section class="rounded-[30px] border border-white/10 bg-[#091626] p-6">
          <div class="flex items-center justify-between gap-4 mb-5">
            <div>
              <p class="text-xs uppercase tracking-[0.22em] text-cyan-400/80 font-semibold">Queue</p>
              <h2 class="text-3xl font-semibold text-slate-50 mt-2">Downloads, processing and pipelines</h2>
            </div>
            <div class="flex items-center gap-3">
              <div class="flex rounded-2xl border border-white/10 bg-white/[0.03] p-1">
                <button
                  @click="queueFilter = 'studio'"
                  class="px-3 py-1.5 rounded-xl text-xs font-semibold transition-colors"
                  :class="queueFilter === 'studio' ? 'bg-cyan-400 text-slate-950' : 'text-slate-300 hover:bg-white/[0.05]'"
                >
                  Current Studio
                </button>
                <button
                  @click="queueFilter = 'all'"
                  class="px-3 py-1.5 rounded-xl text-xs font-semibold transition-colors"
                  :class="queueFilter === 'all' ? 'bg-cyan-400 text-slate-950' : 'text-slate-300 hover:bg-white/[0.05]'"
                >
                  All Jobs
                </button>
              </div>
              <div class="text-sm text-slate-400">
                {{ visibleQueueItems.length }} item{{ visibleQueueItems.length === 1 ? '' : 's' }}
              </div>
              <button
                v-if="queueHistoryCount > 0"
                @click="clearQueueHistory"
                class="inline-flex items-center justify-center whitespace-nowrap h-10 px-4 rounded-2xl border border-white/10 bg-[#060d18] text-slate-200 hover:border-cyan-400/25 transition-colors"
              >
                Clear history
              </button>
            </div>
          </div>

          <div v-if="visibleQueueItems.length === 0" class="py-20 flex flex-col items-center justify-center text-slate-500">
            <FolderOpen class="w-16 h-16 mb-4 opacity-50" />
            <p class="text-lg font-light text-slate-300">No jobs in this view</p>
            <p class="text-sm opacity-70">Switch the filter or add a download, pipeline, or processing job from the other tabs.</p>
          </div>

          <div v-else class="space-y-3">
            <QueueItem
              v-for="item in visibleQueueItems"
              :key="item.id"
              :item="item"
              @cancel="cancelItem"
              @reveal="revealQueueOutput"
              @process="sendToProcess"
            />
          </div>
        </section>
      </template>

      <template v-else>
        <section class="grid grid-cols-1 xl:grid-cols-[1.15fr_0.85fr] gap-5">
          <div class="rounded-[30px] border border-white/10 bg-[#091626] p-6 space-y-5">
            <div>
              <p class="text-xs uppercase tracking-[0.22em] text-cyan-400/80 font-semibold">Process</p>
              <h2 class="text-3xl font-semibold text-slate-50 mt-2">Build the output chain first</h2>
            </div>
            <p class="text-sm text-slate-400 max-w-2xl">
              Select files, choose the export format and define the output destination before moving to the visual split editor.
            </p>
            <button
              @click="activeTab = 'process'"
              class="h-11 px-5 rounded-2xl bg-cyan-400 text-slate-950 font-semibold hover:bg-cyan-300 transition-colors"
            >
              Open Process
            </button>
          </div>

          <div class="rounded-[30px] border border-white/10 bg-[#091626] p-6 space-y-4">
            <div>
              <p class="text-xs uppercase tracking-[0.22em] text-slate-400">State</p>
              <h3 class="text-2xl font-semibold text-slate-50 mt-2">Current snapshot</h3>
            </div>
            <p class="text-sm text-slate-300">{{ processSummary }}</p>
            <p class="text-sm text-slate-400">{{ batchSummary }}</p>
            <p class="text-sm text-slate-400">{{ batchOutputSummary }}</p>
          </div>
        </section>
      </template>

      <Teleport to="body">
        <div
          v-if="activeOverlay"
          class="fixed inset-0 z-[90] bg-[#02060f]/82 backdrop-blur-md p-4 md:p-8"
          @click.self="closeOverlay"
        >
          <div class="mx-auto max-w-3xl max-h-[calc(100vh-2rem)] md:max-h-[calc(100vh-4rem)] overflow-y-auto rounded-[32px] border border-white/10 bg-[#071321] shadow-[0_40px_120px_rgba(0,0,0,0.55)]">
            <div class="sticky top-0 z-10 flex items-start justify-between gap-4 px-6 py-5 border-b border-white/10 bg-[#071321]/95 backdrop-blur-xl">
              <div>
                <p class="text-xs uppercase tracking-[0.24em] text-cyan-400/70">
                  {{
                    activeOverlay === 'downloadOptions'
                      ? 'Download options'
                      : activeOverlay === 'downloadAccess'
                        ? 'Cookies file'
                        : activeOverlay === 'processOutput'
                          ? 'Output settings'
                          : activeOverlay === 'processAudio'
                            ? 'Audio chain'
                            : activeOverlay === 'trimTools'
                              ? 'Trim tools'
                              : 'Tools'
                  }}
                </p>
                <h3 class="text-2xl font-semibold text-slate-50 mt-2">
                  {{
                    activeOverlay === 'downloadOptions'
                      ? 'Tune the download preset'
                      : activeOverlay === 'downloadAccess'
                        ? 'Manage saved cookies.txt'
                        : activeOverlay === 'processOutput'
                          ? 'Choose export and destination'
                          : activeOverlay === 'processAudio'
                            ? 'Shape normalization, EQ and fades'
                            : activeOverlay === 'trimTools'
                              ? 'Set exact trim boundaries'
                              : 'Tune this operation'
                  }}
                </h3>
              </div>

              <button
                @click="closeOverlay"
                class="h-11 w-11 rounded-2xl border border-white/10 bg-[#060d18] text-slate-300 hover:border-cyan-400/20 hover:text-white transition-colors flex items-center justify-center"
              >
                <X class="w-4 h-4" />
              </button>
            </div>

            <div class="p-6 md:p-7 space-y-6">
              <template v-if="activeOverlay === 'downloadOptions'">
                <section class="grid grid-cols-1 md:grid-cols-3 gap-4">
                  <label class="space-y-2">
                    <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Format</span>
                    <select
                      v-model="format"
                      class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                    >
                      <option v-for="downloadFormat in activeDownloadFormats" :key="downloadFormat" :value="downloadFormat">
                        {{ downloadFormat.toUpperCase() }}
                      </option>
                    </select>
                  </label>

                  <label class="space-y-2">
                    <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Quality</span>
                    <select
                      v-model="quality"
                      class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                    >
                      <option
                        v-for="qualityOption in buildDownloadQualityOptions()"
                        :key="qualityOption"
                        :value="qualityOption"
                      >
                        {{ qualityOption === 'best' ? 'Best available' : isAudioDownload ? `${qualityOption} kbps` : `${qualityOption}p` }}
                      </option>
                    </select>
                  </label>

                  <label class="space-y-2">
                    <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Playlist</span>
                    <select
                      v-model="playlistMode"
                      class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                    >
                      <option v-for="mode in PLAYLIST_MODES" :key="mode.value" :value="mode.value">
                        {{ mode.label }}
                      </option>
                    </select>
                  </label>
                </section>

                <section class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-4">
                  <div>
                    <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Destination</p>
                    <p class="text-sm text-slate-300 mt-2">Choose whether downloads stay in the system folder or go to a custom location.</p>
                  </div>

                  <div class="grid grid-cols-1 gap-3">
                    <button
                      @click="downloadDestinationMode = 'downloads'"
                      class="rounded-2xl border px-4 py-4 text-left transition-colors"
                      :class="downloadDestinationMode === 'downloads'
                        ? 'border-cyan-400/35 bg-cyan-400/[0.08] text-white'
                        : 'border-white/10 bg-white/[0.02] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">System Downloads folder</div>
                      <div class="text-xs text-slate-400 mt-1">{{ systemDownloadDir }}</div>
                    </button>

                    <button
                      @click="downloadDestinationMode = 'custom'"
                      class="rounded-2xl border px-4 py-4 text-left transition-colors"
                      :class="downloadDestinationMode === 'custom'
                        ? 'border-cyan-400/35 bg-cyan-400/[0.08] text-white'
                        : 'border-white/10 bg-white/[0.02] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">Custom folder</div>
                      <div class="text-xs text-slate-400 mt-1">{{ downloadPath || 'Choose a destination folder.' }}</div>
                    </button>
                  </div>

                  <button
                    v-if="downloadDestinationMode === 'custom'"
                    @click="selectOutputFolder('download')"
                    class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] hover:border-cyan-400/30 px-4 flex items-center gap-3 text-left"
                  >
                    <FolderOpen class="w-4 h-4 text-slate-500" />
                    <span class="truncate text-sm text-slate-200">{{ downloadPath || 'Choose folder' }}</span>
                  </button>
                </section>

                <section class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-4">
                  <div>
                    <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Note</p>
                    <p class="text-sm text-slate-300 mt-2">Download keeps the source quality. Use the Process tab to apply delivery profiles like AzuraCast or Google TV Streamer Cast.</p>
                  </div>
                </section>
              </template>

              <template v-else-if="activeOverlay === 'downloadAccess'">
                <section class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-4">
                  <div class="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
                    <div class="space-y-2">
                      <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Session access</p>
                      <p class="text-lg font-semibold text-slate-50">{{ savedCookiesStateLabel }}</p>
                      <p class="text-sm text-slate-300 max-w-2xl">{{ savedCookiesHelpText }}</p>
                    </div>

                    <span
                      class="inline-flex items-center rounded-full border px-3 py-2 text-xs font-medium whitespace-nowrap"
                      :class="hasSavedCookiesFile
                        ? 'border-cyan-400/25 bg-cyan-400/10 text-cyan-100'
                        : 'border-white/10 bg-white/[0.04] text-slate-300'"
                    >
                      {{ downloadAccessSummary }}
                    </span>
                  </div>

                  <div class="overflow-hidden rounded-[20px] border border-white/10 bg-[#050c18]">
                    <table class="w-full table-fixed border-collapse">
                      <tbody class="divide-y divide-white/10">
                        <tr class="align-top">
                          <th class="w-[140px] px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">
                            Status
                          </th>
                          <td class="px-4 py-3 text-sm text-slate-100">{{ savedCookiesStateLabel }}</td>
                        </tr>
                        <tr class="align-top">
                          <th class="px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">
                            Storage
                          </th>
                          <td class="px-4 py-3 text-sm text-slate-100">
                            {{ hasSavedCookiesFile ? 'Copied into internal app storage' : 'Nothing imported yet' }}
                          </td>
                        </tr>
                        <tr class="align-top">
                          <th class="px-4 py-3 text-left text-[11px] font-medium uppercase tracking-[0.22em] text-slate-500">
                            Refresh
                          </th>
                          <td class="px-4 py-3 text-sm text-slate-100">
                            {{ hasSavedCookiesFile
                              ? 'Import a fresh cookies.txt if protected downloads start failing'
                              : 'Import a cookies.txt export whenever a source needs login access' }}
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>

                  <div class="flex flex-wrap gap-3">
                    <button
                      @click="selectCookiesFile"
                      class="inline-flex items-center justify-center whitespace-nowrap h-11 px-4 rounded-2xl border border-cyan-400/25 bg-cyan-400/10 text-cyan-100 hover:border-cyan-300/40 hover:bg-cyan-400/15 transition-colors"
                    >
                      {{ savedCookiesActionLabel }}
                    </button>

                    <button
                      v-if="hasSavedCookiesFile"
                      @click="clearSavedCookiesFile"
                      class="inline-flex items-center justify-center whitespace-nowrap h-11 px-4 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-rose-400/30 hover:text-white transition-colors"
                    >
                      Remove saved cookies.txt
                    </button>
                  </div>

                  <div class="rounded-[22px] border border-cyan-400/15 bg-cyan-400/[0.04] p-4">
                    <p class="text-sm text-slate-300 leading-6">
                      The original exported file can change or disappear after import. MediaToolsPro keeps its own internal copy and warns you when that saved session looks outdated.
                    </p>
                  </div>
                </section>
              </template>

              <template v-else-if="activeOverlay === 'processOutput'">
                <section class="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <label class="space-y-2">
                    <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Export format</span>
                    <select
                      v-model="batchFormat"
                      class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                    >
                      <option v-for="exportFormat in activeBatchFormats" :key="exportFormat" :value="exportFormat">
                        {{ exportFormat.toUpperCase() }}
                      </option>
                    </select>
                  </label>
                </section>

                <section class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-4">
                  <div>
                    <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Destination</p>
                    <p class="text-sm text-slate-300 mt-2">Choose whether outputs stay next to the original files or go to one shared folder.</p>
                  </div>

                  <div class="grid grid-cols-1 gap-3">
                    <button
                      @click="batchDestinationMode = 'source'"
                      class="rounded-2xl border px-4 py-4 text-left transition-colors"
                      :class="batchDestinationMode === 'source'
                        ? 'border-cyan-400/35 bg-cyan-400/[0.08] text-white'
                        : 'border-white/10 bg-[#050c18] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">Same folder as source</div>
                      <div class="text-xs text-slate-400 mt-1">{{ batchOutputSummary }}</div>
                    </button>

                    <button
                      @click="batchDestinationMode = 'custom'"
                      class="rounded-2xl border px-4 py-4 text-left transition-colors"
                      :class="batchDestinationMode === 'custom'
                        ? 'border-cyan-400/35 bg-cyan-400/[0.08] text-white'
                        : 'border-white/10 bg-[#050c18] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">Custom output folder</div>
                      <div class="text-xs text-slate-400 mt-1">{{ batchOutputDir || 'Choose one shared destination.' }}</div>
                    </button>
                  </div>

                  <button
                    v-if="batchDestinationMode === 'custom'"
                    @click="selectOutputFolder('batch')"
                    class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] hover:border-cyan-400/30 px-4 flex items-center gap-3 text-left"
                  >
                    <FolderOpen class="w-4 h-4 text-slate-500" />
                    <span class="truncate text-sm text-slate-200">{{ batchOutputDir || 'Choose folder' }}</span>
                  </button>
                </section>
              </template>

              <template v-else-if="activeOverlay === 'processAudio'">
                <section v-if="isAudioBatch" class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-4">
                  <div>
                    <p class="text-xs uppercase tracking-[0.22em] text-slate-400">Delivery profile</p>
                    <p class="text-sm text-slate-300 mt-2">Select the target and then fine tune loudness and EQ.</p>
                  </div>
                  <div class="grid grid-cols-1 gap-3">
                    <button
                      v-for="target in AUDIO_TARGETS"
                      :key="target.value"
                      @click="batchAudioTarget = target.value"
                      class="rounded-2xl border px-4 py-4 text-left transition-colors"
                      :class="batchAudioTarget === target.value
                        ? 'border-cyan-400/35 bg-cyan-400/[0.08] text-white'
                        : 'border-white/10 bg-white/[0.02] text-slate-300 hover:border-cyan-400/20'"
                    >
                      <div class="text-sm font-medium">{{ target.label }}</div>
                      <div class="text-xs text-slate-400 mt-1">{{ target.hint }}</div>
                    </button>
                  </div>
                </section>

                <section class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-5">
                  <label class="flex items-center gap-3 rounded-2xl border border-white/10 bg-[#050c18] px-4 py-4">
                    <input
                      v-model="normalizeAudio"
                      type="checkbox"
                      class="w-4 h-4 rounded border-slate-500 text-cyan-400 focus:ring-cyan-400"
                    />
                    <div>
                      <p class="text-sm font-medium text-slate-100">Normalize loudness</p>
                      <p class="text-xs text-slate-400 mt-1">Applies `loudnorm` to each file or segment at {{ loudnessTargetLufs }} LUFS.</p>
                    </div>
                  </label>

                  <div
                    v-if="normalizeAudio"
                    class="rounded-2xl border border-white/10 bg-[#050c18] p-4 space-y-4"
                  >
                    <div>
                      <p class="text-xs uppercase tracking-[0.18em] text-slate-400">Loudness target</p>
                      <p class="text-sm text-slate-300 mt-2">Choose a classic LUFS preset or enter a manual target.</p>
                    </div>

                    <div class="grid grid-cols-2 md:grid-cols-4 gap-2">
                      <button
                        v-for="preset in LOUDNESS_PRESETS"
                        :key="preset.value"
                        @click="loudnessTargetLufs = preset.value"
                        class="rounded-2xl border px-3 py-3 text-left transition-colors"
                        :class="loudnessTargetLufs === preset.value
                          ? 'border-cyan-400/35 bg-cyan-400/[0.08] text-white'
                          : 'border-white/10 bg-white/[0.02] text-slate-300 hover:border-cyan-400/20'"
                      >
                        <div class="text-sm font-medium">{{ preset.value }} LUFS</div>
                        <div class="text-[11px] text-slate-400 mt-1">{{ preset.label }}</div>
                      </button>
                    </div>

                    <label class="space-y-2 block">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Manual LUFS</span>
                      <input
                        v-model.number="loudnessTargetLufs"
                        type="number"
                        min="-30"
                        max="-6"
                        step="0.5"
                        class="w-full h-11 rounded-2xl border border-white/10 bg-[#060d18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                      />
                    </label>
                  </div>

                  <div class="grid grid-cols-1 gap-4">
                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Bass {{ eq.bass }} dB</span>
                      <input v-model.number="eq.bass" type="range" min="-12" max="12" step="0.5" class="w-full accent-cyan-400" />
                    </label>
                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Mid {{ eq.mid }} dB</span>
                      <input v-model.number="eq.mid" type="range" min="-12" max="12" step="0.5" class="w-full accent-cyan-400" />
                    </label>
                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Treble {{ eq.treble }} dB</span>
                      <input v-model.number="eq.treble" type="range" min="-12" max="12" step="0.5" class="w-full accent-cyan-400" />
                    </label>
                  </div>

                  <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Fade in</span>
                      <input
                        v-model.number="fadeInDuration"
                        type="number"
                        min="0"
                        step="0.1"
                        class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                      />
                    </label>

                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Fade out</span>
                      <input
                        v-model.number="fadeOutDuration"
                        type="number"
                        min="0"
                        step="0.1"
                        class="w-full h-11 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                      />
                    </label>
                  </div>
                </section>
              </template>

              <template v-else-if="activeOverlay === 'trimTools'">
                <section class="rounded-[26px] border border-white/10 bg-[#060d18] p-5 space-y-5">
                  <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3">
                    <button
                      @click="setTrimFromCursor('start')"
                      class="h-11 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                    >
                      Use cursor as start
                    </button>
                    <button
                      @click="setTrimFromCursor('end')"
                      class="h-11 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                    >
                      Use cursor as end
                    </button>
                    <button
                      v-if="isManualSplitMode"
                      @click="addMarkerAtCursor"
                      class="h-11 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                    >
                      Add mark at cursor
                    </button>
                    <button
                      @click="resetTrimSelection"
                      class="h-11 rounded-2xl border border-white/10 bg-[#050c18] text-slate-200 hover:border-cyan-400/20 transition-colors"
                    >
                      Reset trim
                    </button>
                  </div>

                  <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Trim start</span>
                      <input
                        v-model="trimStart"
                        type="text"
                        placeholder="00:00 or seconds"
                        class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 placeholder:text-slate-500 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                      />
                    </label>

                    <label class="space-y-2">
                      <span class="text-xs uppercase tracking-[0.18em] text-slate-400">Trim end</span>
                      <input
                        v-model="trimEnd"
                        type="text"
                        placeholder="Leave empty to keep the end"
                        class="w-full h-12 rounded-2xl border border-white/10 bg-[#050c18] px-4 text-slate-100 placeholder:text-slate-500 focus:outline-none focus:ring-1 focus:ring-cyan-400/50"
                      />
                    </label>
                  </div>
                </section>
              </template>

            </div>
          </div>
        </div>
      </Teleport>
    </div>
  </div>
</template>
