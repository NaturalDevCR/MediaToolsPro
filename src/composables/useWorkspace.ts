import { computed, ref } from 'vue';
import { addLog as pushLog } from '../stores/logs';
import type {
  AudioTarget,
  MediaKind,
  PlaylistMode,
  SplitMode,
} from '../types/jobs';

/**
 * Cross-cutting workspace state shared by the download and studio surfaces:
 * the active tab, the studio (audio/video) mode, the resolved system download
 * directory, and the modal overlay host. Also exposes the shared logging and
 * path helpers so every composable formats logs and paths identically.
 *
 * State lives at module scope so every `useWorkspace()` caller shares the same
 * singleton reactive objects (no prop-drilling, no provide/inject).
 */

export type WorkspaceTab = 'downloads' | 'process' | 'split' | 'queue';
export type QueueFilter = 'studio' | 'all';
export type StudioMode = MediaKind;
export type SummaryRow = { label: string; value: string; detail?: string };
export type OverlayPanel =
  | 'downloadOptions'
  | 'downloadAccess'
  | 'processOutput'
  | 'processAudio'
  | 'trimTools';

export const AUDIO_BITRATES = ['320', '256', '192', '128'];
export const VIDEO_QUALITIES = ['2160', '1080', '720', '480'];

export const TABS: Array<{ value: WorkspaceTab; label: string; hint: string }> = [
  { value: 'downloads', label: 'Downloads', hint: 'URL, playlist, cookies' },
  { value: 'process', label: 'Process', hint: 'Export, normalize, EQ' },
  { value: 'split', label: 'Split / Trim', hint: 'Waveform, marks, silence' },
  { value: 'queue', label: 'Queue', hint: 'Track every job' },
];

export const AUDIO_TARGETS: Array<{ value: AudioTarget; label: string; hint: string }> = [
  { value: 'general', label: 'General audio', hint: 'Normal download and export behavior.' },
  {
    value: 'azuracast',
    label: 'AzuraCast Ready',
    hint: 'MP3, 320 kbps, 44.1 kHz stereo and offline loudness prep.',
  },
];

export const LOUDNESS_PRESETS = [
  { value: -14, label: 'Streaming loud' },
  { value: -16, label: 'Podcast / web' },
  { value: -18, label: 'Broadcast light' },
  { value: -23, label: 'EBU R128' },
];

export const PLAYLIST_MODES: Array<{ value: PlaylistMode; label: string; hint: string }> = [
  { value: 'auto', label: 'Auto detect', hint: 'Use the full playlist when the URL includes one.' },
  { value: 'playlist', label: 'Full playlist', hint: 'Force every item in the playlist to download.' },
  { value: 'single', label: 'Single item', hint: 'Ignore playlist context and fetch only one item.' },
];

export const SPLIT_MODES: Array<{ value: SplitMode; label: string; hint: string }> = [
  { value: 'none', label: 'Trim only', hint: 'Keep one output and only trim it.' },
  { value: 'silence', label: 'Auto split', hint: 'Cut long mixes by silence detection.' },
  {
    value: 'chapters',
    label: 'Source chapters',
    hint: 'Use embedded YouTube chapters, falling back to silence.',
  },
  { value: 'manual', label: 'Manual marks', hint: 'Use your own cut points on the timeline.' },
];

const activeTab = ref<WorkspaceTab>('downloads');
const studioMode = ref<StudioMode>('audio');
const systemDownloadDir = ref('Downloads');
const activeOverlay = ref<OverlayPanel | null>(null);

const isAudioStudio = computed(() => studioMode.value === 'audio');

const addLog = (
  message: string,
  level: 'info' | 'warn' | 'error' | 'success' = 'info',
  scope: 'audio' | 'video' | 'system' = studioMode.value,
) => pushLog(message, level, scope);

const generateId = () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;

function formatBytes(bytes: number) {
  if (!bytes || bytes === 0) return 'Unknown size';
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${sizes[i]}`;
}

const basename = (value: string) => value.split(/[/\\]/).pop() || value;
const dirname = (value: string) => {
  const normalized = value.replace(/\\/g, '/');
  const segments = normalized.split('/');
  segments.pop();
  return segments.join('/') || '';
};

function openOverlay(panel: OverlayPanel) {
  activeOverlay.value = panel;
}

function closeOverlay() {
  activeOverlay.value = null;
}

export function useWorkspace() {
  return {
    activeTab,
    studioMode,
    systemDownloadDir,
    activeOverlay,
    isAudioStudio,
    addLog,
    generateId,
    formatBytes,
    basename,
    dirname,
    openOverlay,
    closeOverlay,
  };
}
