import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type {
  AudioTarget,
  DownloadJobRequest,
  PlaylistMode,
  VideoTarget,
  YtdlpFormatItem,
  YtdlpFormatsResponse,
} from '../types/jobs';
import { AUDIO_FORMATS, VIDEO_FORMATS, type DownloadDestinationMode } from './useSettings';
import { buildDownloadDetail, useQueue } from './useQueue';
import {
  AUDIO_BITRATES,
  PLAYLIST_MODES,
  VIDEO_QUALITIES,
  useWorkspace,
  type SummaryRow,
} from './useWorkspace';

/**
 * Singleton owner of the download surface: the URL → format-explorer → confirm
 * flow, the export preset (format/quality/playlist), the destination,
 * the saved-cookies session, and the action that queues a download job. State
 * lives at module scope so every caller shares the same reactive objects.
 */

const { activeTab, systemDownloadDir, isAudioStudio, addLog, generateId, basename } = useWorkspace();
const { queue, processQueue } = useQueue();

const url = ref('');
const format = ref('mp3');
const quality = ref('best');
const playlistMode = ref<PlaylistMode>('auto');
const downloadAudioTarget = ref<AudioTarget>('general');
const downloadVideoTarget = ref<VideoTarget>('general');
const downloadDestinationMode = ref<DownloadDestinationMode>('downloads');
const downloadPath = ref('');
const cookiesFile = ref('');

// Format explorer state
const downloadStep = ref<'url' | 'formats' | 'confirm'>('url');
const formatList = ref<YtdlpFormatItem[]>([]);
const selectedFormatId = ref<string | null>(null);
const formatListLoading = ref(false);
const formatListError = ref('');
const formatListTitle = ref('');

const activeDownloadFormats = computed(() => (isAudioStudio.value ? AUDIO_FORMATS : VIDEO_FORMATS));
const isAudioDownload = computed(() => isAudioStudio.value && AUDIO_FORMATS.includes(format.value));

const selectedFormat = computed<YtdlpFormatItem | null>(() => {
  if (!selectedFormatId.value) return null;
  return formatList.value.find((f) => f.formatId === selectedFormatId.value) ?? null;
});

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
const downloadReady = computed(() => {
  if (!url.value.trim()) {
    return false;
  }

  if (downloadDestinationMode.value === 'custom' && !downloadPath.value) {
    return false;
  }

  return true;
});
const canConfirmDownload = computed(() => {
  return selectedFormatId.value !== null && downloadReady.value;
});
const hasSavedCookiesFile = computed(() => Boolean(cookiesFile.value.trim()));
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

function buildDownloadQualityOptions() {
  if (isAudioDownload.value) {
    return ['best', ...AUDIO_BITRATES];
  }

  return ['best', ...VIDEO_QUALITIES];
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

function buildCurrentDownloadSettings(outputPath: string): Omit<DownloadJobRequest, 'kind' | 'url'> {
  return {
    format: format.value,
    quality: quality.value,
    formatId: selectedFormatId.value ?? undefined,
    formatHasAudio: selectedFormat.value?.hasAudio,
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

export function useDownloads() {
  return {
    url,
    format,
    quality,
    playlistMode,
    downloadAudioTarget,
    downloadVideoTarget,
    downloadDestinationMode,
    downloadPath,
    cookiesFile,
    downloadStep,
    formatList,
    selectedFormatId,
    formatListLoading,
    formatListError,
    formatListTitle,
    activeDownloadFormats,
    isAudioDownload,
    selectedFormat,
    resolvedDownloadPath,
    downloadFormatSummary,
    downloadAccessSummary,
    savedCookiesStateLabel,
    savedCookiesActionLabel,
    savedCookiesHelpText,
    currentPlaylistModeHint,
    downloadReady,
    canConfirmDownload,
    hasSavedCookiesFile,
    downloadSummaryRows,
    buildDownloadQualityOptions,
    restoreSavedCookiesFile,
    clearSavedCookiesFile,
    ensureSavedCookiesFileAvailable,
    selectCookiesFile,
    buildCurrentDownloadSettings,
    exploreFormats,
    resetFormatExplorer,
    addDownloadToQueue,
  };
}
