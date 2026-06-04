import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  AudioTarget,
  DownloadJobRequest,
  JobProgressPayload,
  JobRequest,
  PlaylistMode,
  ProcessJobRequest,
  QueueItemData,
  QueueStatus,
  SplitMode,
  VideoTarget,
} from '../types/jobs';
import {
  PLAYLIST_MODES,
  useWorkspace,
  type QueueFilter,
} from './useWorkspace';

export function isTerminalStatus(status: QueueStatus) {
  return ['done', 'error', 'cancelled'].includes(status);
}

export function isActiveStatus(status: QueueStatus) {
  return ['downloading', 'processing', 'converting'].includes(status);
}

export function getQueueStats(items: QueueItemData[]) {
  const active = items.filter((item) => isActiveStatus(item.status)).length;
  const waiting = items.filter((item) => item.status === 'waiting').length;
  return { total: items.length, active, waiting };
}

export function applyJobProgress(item: QueueItemData, payload: JobProgressPayload) {
  item.kind = payload.jobKind;
  item.mediaKind = payload.mediaKind;
  item.status = payload.status;
  item.percent = payload.percent;
  item.speed = payload.speed;
  item.eta = payload.eta;
  item.totalSize = payload.totalSize;

  if (payload.title) {
    item.title = payload.title;
  }

  if (payload.detail) {
    item.detail = payload.detail;
  }

  if (payload.outputPath) {
    item.outputPath = payload.outputPath;
  }

  if (payload.error) {
    item.error = payload.error;
  }
}

const DOWNLOAD_CONCURRENT_LIMIT = 2;
const PROCESS_CONCURRENT_LIMIT = 2;
const QUEUE_STORAGE_KEY = 'mediatoolspro.queue.v1';
const LEGACY_QUEUE_STORAGE_KEY = 'audiotoolspro.queue.v1';

const { addLog, generateId, studioMode } = useWorkspace();

const queue = ref<QueueItemData[]>([]);
const queueFilter = ref<QueueFilter>('studio');

let queuePersistenceTimer: number | null = null;

const queueStats = computed(() => getQueueStats(queue.value));
const visibleQueueItems = computed(() =>
  queueFilter.value === 'all'
    ? queue.value
    : queue.value.filter((item) => item.mediaKind === studioMode.value),
);
const queueHistoryCount = computed(() =>
  queue.value.filter((item) => isTerminalStatus(item.status)).length,
);

function getActiveCount(kind: QueueItemData['kind']) {
  return queue.value.filter((item) => item.kind === kind && isActiveStatus(item.status)).length;
}

function getBackendJobId(item: QueueItemData) {
  return item.activeBackendId ?? item.id;
}

export function buildDownloadDetail(request: DownloadJobRequest) {
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

export function buildProcessDetail(request: ProcessJobRequest) {
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

export function shouldWarnCookiesFileFailure(message: string) {
  return /cookies\.txt may be expired|export a fresh cookies\.txt|saved internal cookies\.txt/i.test(
    message,
  );
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

function clearQueuePersistenceTimer() {
  if (queuePersistenceTimer !== null) {
    window.clearTimeout(queuePersistenceTimer);
  }
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

/**
 * Singleton owner of the job queue: the queue array, concurrency-limited
 * scheduling, the per-job lifecycle (download / process / download+process
 * pipeline), cancellation, output reveal, history persistence/restore and the
 * queue-derived computeds. The pure helpers above stay exported for unit tests
 * and for the form composables that build queue items.
 */
export function useQueue() {
  return {
    queue,
    queueFilter,
    queueStats,
    visibleQueueItems,
    queueHistoryCount,
    processQueue,
    cancelItem,
    revealQueueOutput,
    updateQueueItem,
    persistQueueHistory,
    scheduleQueuePersistence,
    clearQueuePersistenceTimer,
    restoreQueueHistory,
    clearQueueHistory,
    buildDownloadDetail,
    buildProcessDetail,
  };
}
