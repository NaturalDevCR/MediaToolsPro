<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import { Channel, convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  ArrowRight,
  AudioLines,
  Check,
  ChevronRight,
  FileAudio,
  FolderOpen,
  Layers,
  ListOrdered,
  LoaderCircle,
  Minus,
  Pause,
  Play,
  Plus,
  Repeat2,
  RotateCcw,
  Scissors,
  Split,
  Square,
  Upload,
  X,
} from "lucide-vue-next";
import QueueItem from "../components/QueueItem.vue";
import SplitComparisonPlayer from "../components/SplitComparisonPlayer.vue";
import ExportOptions from "../components/ExportOptions.vue";
import { compatibleEncoding } from "../lib/export-options";
import { t, tMediaMessage } from "../lib/i18n";
import { formatPlaybackTime as time } from "../lib/playback";
import {
  basename,
  cancelItem,
  clearHistory,
  enqueue,
  pendingCount,
  queue,
  queueReady,
  retryItem,
  revealOutput,
} from "../stores/studio";
import type {
  AutomaticSilence,
  ExportEncoding,
  MediaProbeResponse,
  ProcessJobRequest,
  Segment,
  SilenceSensitivity,
  SplitComparison,
  SplitComparisonUpdate,
  SplitMode,
  SplitNormalization,
  SplitPlan,
  Tool,
  WaveformPreviewResponse,
  WaveformLevelsResponse,
} from "../types/jobs";

import {
  createWaveformEnvelope,
  dbHeight,
  inspectLevel,
  quietRegions,
  suggestThreshold,
  waveformRulerTicks,
  waveformScrollOffset,
  waveformTrackWidth,
  waveformVisibleRange,
  waveformZoomSpans,
} from "../lib/waveform";
import {
  audioFormats,
  isSplitPreset,
  parseSplitPresets,
  splitPresetsKey,
  validOutputNamePattern,
  videoFormats,
  type SplitPreset,
} from "../lib/split-presets";

const props = defineProps<{ active: boolean }>();

type Source = { path: string; probe?: MediaProbeResponse; error?: string };
const tools = [
  {
    id: "trim" as Tool,
    name: "Trim",
    icon: Scissors,
    hint: "Conserva lo que importa",
    description: "Un inicio preciso. Un final limpio.",
  },
  {
    id: "split" as Tool,
    name: "Split",
    icon: Split,
    hint: "De una pieza a muchas",
    description: "Encuentra cada corte. Revisa cada segmento.",
  },
  {
    id: "normalize" as Tool,
    name: "Normalize",
    icon: AudioLines,
    hint: "Un nivel consistente",
    description: "Sonoridad medida. Exportaciones consistentes.",
  },
];
const tool = ref<Tool>("trim");
const showQueue = ref(false);
const sources = ref<Source[]>([]);
const selectedPath = ref("");
const selected = computed(() =>
  sources.value.find((source) => source.path === selectedPath.value),
);
const duration = computed(() => selected.value?.probe?.durationSeconds || 0);
const currentTool = computed(
  () => tools.find((item) => item.id === tool.value)!,
);
const preview = ref<WaveformPreviewResponse>();
const previewBusy = ref(false);
const previewError = ref("");
const error = ref("");
const notice = ref("");
const dragging = ref(false);
const audio = ref<HTMLAudioElement>();
const timeline = ref<HTMLElement>();
const waveScroll = ref<HTMLElement>();
const waveCanvas = ref<HTMLCanvasElement>();
const levels = shallowRef<WaveformLevelsResponse>();
const levelsBusy = ref(false);
const levelsError = ref("");
const hoverTime = ref<number | null>(null);
const lastInspectedTime = ref<number | null>(null);
const scrollOffset = ref(0);
const viewportWidth = ref(0);
const zoomSpan = ref<number>();
const jumpTime = ref("");
const jumpInvalid = ref(false);
const zoomSpans = computed(() =>
  waveformZoomSpans(duration.value, viewportWidth.value),
);
const activeZoom = computed({
  get: () => {
    const requested = zoomSpan.value ?? duration.value;
    const spans = zoomSpans.value;
    return spans.find((span) => span <= requested) ?? spans[spans.length - 1];
  },
  set: (span: number) => void changeZoom(span),
});
const zoomIndex = computed(() => zoomSpans.value.indexOf(activeZoom.value));
const trackWidth = computed(() =>
  waveformTrackWidth(duration.value, activeZoom.value, viewportWidth.value),
);
const visibleRange = computed(() =>
  waveformVisibleRange(
    duration.value,
    trackWidth.value,
    viewportWidth.value,
    scrollOffset.value,
  ),
);
const envelope = computed(() => levels.value && createWaveformEnvelope(levels.value));
let levelsToken = 0;
let levelsId = "";
let finishLevelsTransfer: (() => void) | undefined;
let resizeObserver: ResizeObserver | undefined;
let paintFrame = 0;
const quiet = computed(() =>
  automaticAnalysis.value
    ? automaticAnalysis.value.pauses
    : levels.value
    ? quietRegions(levels.value, effectiveThreshold.value, duration.value)
    : [],
);
const inspectionTime = computed(
  () =>
    hoverTime.value ??
    (playing.value ? cursor.value : (lastInspectedTime.value ?? cursor.value)),
);
const inspectedLevel = computed(() =>
  levels.value ? inspectLevel(levels.value, inspectionTime.value) : null,
);
const inspectedSilence = computed(() =>
  quiet.value.find(
    (region) =>
      inspectionTime.value >= region.startSeconds &&
      inspectionTime.value < region.endSeconds,
  ),
);
const suggestedThreshold = computed(() =>
  inspectedLevel.value ? suggestThreshold(inspectedLevel.value.peakDb) : null,
);
const silenceDuration = computed(() =>
  inspectedSilence.value
    ? inspectedSilence.value.endSeconds - inspectedSilence.value.startSeconds
    : 0,
);
const tooltipLeft = computed(
  () =>
    scrollOffset.value +
    Math.max(
      75,
      Math.min(
        viewportWidth.value - 75,
        (percent(inspectionTime.value) / 100) *
          (timeline.value?.clientWidth || 0) -
          scrollOffset.value,
      ),
    ),
);
const rulerTicks = computed(() => {
  if (!duration.value || !viewportWidth.value) return [{ seconds: 0, label: "00:00" }];
  const { startSeconds, endSeconds } = visibleRange.value;
  const { step, seconds } = waveformRulerTicks(
    startSeconds,
    endSeconds,
    viewportWidth.value,
  );
  return seconds.map((value) => ({
    seconds: value,
    label: step < 1 ? time(value) : time(value).replace(/\.\d+$/, ""),
  }));
});

function dbLabel(db: number) {
  return db <= -120 ? "−∞" : db.toFixed(1);
}
function schedulePaint() {
  cancelAnimationFrame(paintFrame);
  paintFrame = requestAnimationFrame(paintWaveform);
}
function waveScrolled() {
  scrollOffset.value = waveScroll.value?.scrollLeft || 0;
  hoverTime.value = null;
  schedulePaint();
}
function paintWaveform() {
  const canvas = waveCanvas.value;
  const scroll = waveScroll.value;
  const track = timeline.value;
  if (!scroll?.clientWidth || !track) return;
  const width = scroll.clientWidth;
  if (width !== viewportWidth.value) {
    const oldWidth = viewportWidth.value;
    const focus = waveFocus(oldWidth, track.clientWidth, scrollOffset.value);
    viewportWidth.value = width;
    void nextTick(() => {
      if (oldWidth) scrollWaveTo(focus.seconds, (focus.screenX / oldWidth) * width);
      else waveScrolled();
    });
    return;
  }
  const measure = envelope.value;
  if (!canvas || !duration.value || !measure) return;
  const height = track.clientHeight - 30;
  const ratio = Math.min(2, window.devicePixelRatio || 1);
  const offset = scroll.scrollLeft;
  scrollOffset.value = offset;
  const pixelWidth = Math.round(width * ratio);
  const pixelHeight = Math.round(height * ratio);
  if (canvas.width !== pixelWidth) canvas.width = pixelWidth;
  if (canvas.height !== pixelHeight) canvas.height = pixelHeight;
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  canvas.style.left = `${offset}px`;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const center = height / 2;
  const half = center - 8;
  const secondsPerPixel = duration.value / track.clientWidth;
  if (tool.value === "split" && usesPauses.value) {
    ctx.fillStyle = "#d6ad5d15";
    for (const region of quiet.value) {
      if (region.endSeconds - region.startSeconds < minimumVisiblePause.value) continue;
      const left =
        (region.startSeconds / duration.value) * track.clientWidth - offset;
      const right =
        (region.endSeconds / duration.value) * track.clientWidth - offset;
      if (right >= 0 && left <= width)
        ctx.fillRect(
          Math.max(0, left),
          0,
          Math.min(width, right) - Math.max(0, left),
          height,
        );
    }
  }
  const peaks = new Path2D();
  const rms = new Path2D();
  for (let x = 0; x < width; x++) {
    const { peakDb, rmsDb } = measure(
      (offset + x) * secondsPerPixel,
      (offset + x + 1) * secondsPerPixel,
    );
    const amplitude = Math.max(0.5, dbHeight(peakDb) * half);
    const body = Math.max(0.3, dbHeight(rmsDb) * half);
    peaks.rect(x, center - amplitude, 1, amplitude * 2);
    rms.rect(x, center - body, 1, body * 2);
  }
  ctx.fillStyle = "#4b9c7e";
  ctx.fill(peaks);
  ctx.fillStyle = "#93d8b4";
  ctx.fill(rms);
  if (
    tool.value === "split" &&
    usesPauses.value &&
    (!autoSilence.value || automaticAnalysis.value) &&
    Number.isFinite(effectiveThreshold.value)
  ) {
    const h = dbHeight(effectiveThreshold.value) * half;
    ctx.strokeStyle = "#ecc681bb";
    ctx.setLineDash([4, 4]);
    ctx.beginPath();
    ctx.moveTo(0, center - h);
    ctx.lineTo(width, center - h);
    ctx.moveTo(0, center + h);
    ctx.lineTo(width, center + h);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.font = "9px ui-monospace, monospace";
    ctx.fillStyle = "#ecc681";
    ctx.fillText(`${effectiveThreshold.value} dBFS`, 8, Math.max(12, center - h - 5));
  }
}
function waveFocus(width: number, track: number, offset: number) {
  if (!width || !track || !duration.value)
    return { seconds: cursor.value, screenX: 0 };
  const cursorX = (cursor.value / duration.value) * track - offset;
  if (cursorX >= 0 && cursorX <= width)
    return { seconds: cursor.value, screenX: cursorX };
  const screenX = width / 2;
  return { seconds: ((offset + screenX) / track) * duration.value, screenX };
}
function scrollWaveTo(seconds: number, screenX: number) {
  const scroll = waveScroll.value;
  const track = timeline.value;
  if (!scroll || !track) return;
  scroll.scrollLeft = waveformScrollOffset(
    seconds,
    screenX,
    duration.value,
    track.clientWidth,
    scroll.clientWidth,
  );
  waveScrolled();
}
async function changeZoom(span: number) {
  const scroll = waveScroll.value;
  const track = timeline.value;
  if (!scroll || !track || !zoomSpans.value.includes(span)) return;
  const focus = waveFocus(scroll.clientWidth, track.clientWidth, scroll.scrollLeft);
  zoomSpan.value = span === duration.value ? undefined : span;
  await nextTick();
  scrollWaveTo(focus.seconds, focus.screenX);
}
function zoomLabel(span: number) {
  if (span === duration.value) return t("Vista completa");
  if (span >= 3600 && span % 3600 === 0) return `${span / 3600} h`;
  if (span >= 60 && span % 60 === 0) return `${span / 60} min`;
  return `${Number(span.toFixed(span < 1 ? 3 : 1))} s`;
}
function jumpToTime() {
  const seconds = parse(jumpTime.value.replace(",", "."));
  jumpInvalid.value = seconds === null || seconds > duration.value + 0.0005;
  if (jumpInvalid.value || seconds === null) return;
  seek(seconds);
  jumpTime.value = time(cursor.value);
  scrollWaveTo(cursor.value, (waveScroll.value?.clientWidth || 0) / 2);
}
async function cancelLevels() {
  levelsToken++;
  levelsBusy.value = false;
  finishLevelsTransfer?.();
  finishLevelsTransfer = undefined;
  if (levelsId) {
    const id = levelsId;
    levelsId = "";
    try {
      await invoke("cancel_job", { id });
    } catch {
      /* Finished. */
    }
  }
}
async function loadLevels(path: string) {
  await cancelLevels();
  if (selectedPath.value !== path || disposed) return;
  const token = ++levelsToken;
  const id = crypto.randomUUID();
  levelsId = id;
  levelsBusy.value = true;
  const peaksDb: number[] = [];
  const rmsDb: number[] = [];
  const transferComplete = new Promise<void>((resolve) => {
    finishLevelsTransfer = resolve;
  });
  const onChunk = new Channel<Pick<WaveformLevelsResponse, "peaksDb" | "rmsDb"> | null>(
    (chunk) => {
      if (token !== levelsToken || disposed) return;
      if (chunk === null) {
        finishLevelsTransfer?.();
        return;
      }
      peaksDb.push(...chunk.peaksDb);
      rmsDb.push(...chunk.rmsDb);
    },
  );
  try {
    const [intervalSeconds] = await Promise.all([
      invoke<number>("analyze_waveform", { inputPath: path, id, onChunk }),
      transferComplete,
    ]);
    if (token !== levelsToken || disposed) return;
    levels.value = { intervalSeconds, peaksDb, rmsDb };
    await nextTick();
    schedulePaint();
  } catch (e) {
    if (token === levelsToken && !disposed) levelsError.value = String(e);
  } finally {
    if (token === levelsToken) {
      levelsBusy.value = false;
      levelsId = "";
      finishLevelsTransfer = undefined;
    }
  }
}
const cursor = ref(0);
const playing = ref(false);
const playbackStarting = ref(false);
const playbackActive = computed(() => playing.value || playbackStarting.value);
const playbackSegment = shallowRef<Segment>();
const loop = ref(false);
const start = ref("00:00.000");
const end = ref("");
const trimFades = ref({ in: 0, out: 0 });
const splitFades = ref({ in: 0, out: 0 });
const fades = computed(() =>
  tool.value === "split" ? splitFades.value : trimFades.value,
);
const splitMode = ref<Exclude<SplitMode, "none">>("silence");
const splitDuration = ref("03:00");
const splitDurationSeconds = computed(() => parse(splitDuration.value));
const snapToPause = ref(false);
const pauseWindow = ref(5);
const snapMinSilence = ref(0.3);
const autoSilence = ref(true);
const scanAutomatically = ref(false);
const silenceSensitivity = ref<SilenceSensitivity>("balanced");
const automaticAnalysis = ref<AutomaticSilence>();
const splitOptions = ref<HTMLElement>();
const usesPauses = computed(() =>
  splitMode.value === "silence" || (splitMode.value === "duration" && snapToPause.value),
);
const effectiveThreshold = computed(() => automaticAnalysis.value?.thresholdDb ?? thresholdDb.value);
const minimumVisiblePause = computed(() =>
  automaticAnalysis.value ? 0 : splitMode.value === "duration" ? snapMinSilence.value : minSilence.value,
);
let analysisTimer: ReturnType<typeof setTimeout> | undefined;
const outputNamePattern = ref("");
const storedPresets = ref(parseSplitPresets(localStorage.getItem(splitPresetsKey)));
const presetChoice = ref("");
const presetName = ref("");
const presetStatus = ref("");
const comparison = ref<SplitComparison>();
const comparisonSegment = ref<Segment>();
const comparisonIndex = ref(0);
const comparisonPending = ref(false);
const comparisonPercent = ref(0);
const comparisonError = ref("");
const comparisonPlayer = ref<InstanceType<typeof SplitComparisonPlayer>>();
const comparisonPanel = ref<HTMLElement>();
let comparisonId = "";
const splitNormalization = ref<SplitNormalization>("none");
const splitModes = {
  chapters: {
    label: "Por capítulos",
    help: "Usa los capítulos incrustados en el archivo local.",
    action: "Leer capítulos",
  },
  silence: {
    label: "Por silencios",
    help: "Coloca un corte en el centro de cada silencio que cumpla los parámetros.",
    action: "Detectar silencios",
  },
  duration: {
    label: "Por duración",
    help: "Divide desde el inicio de la selección y conserva el tramo final aunque sea más corto.",
    action: "Generar segmentos",
  },
  manual: {
    label: "Manual",
    help: "Haz doble clic en la onda, usa el cursor o escribe un tiempo exacto para añadir cada corte.",
  },
};
const splitNormalizationOptions = {
  none: { label: "Desactivada", help: "" },
  before: {
    label: "Antes de dividir",
    help: "Mide y ajusta toda la selección en conjunto. Los fades se aplican al exportar cada segmento.",
  },
  after: {
    label: "Después de dividir",
    help: "Mide y ajusta cada segmento por separado con sus fades ya aplicados.",
  },
};
const markers = ref<number[]>([]);
const markerInput = ref("");
const thresholdDb = ref(-35);
const minSilence = ref(1.5);
const minSegment = ref(20);
const analysisBusy = ref(false);
const analyzedSegments = ref<Segment[]>([]);
const format = ref("wav");
const encodingByFormat = ref<Record<string, ExportEncoding>>({});
const currentEncoding = computed({
  get: () => encodingByFormat.value[format.value] ?? {},
  set: (encoding: ExportEncoding) => { encodingByFormat.value[format.value] = encoding; },
});
const destination = ref("source");
const outputDir = ref("");
const loudness = ref(-16);
const truePeak = ref(-1.5);
const loudnessValid = computed(
  () =>
    Number.isFinite(loudness.value) &&
    loudness.value >= -30 &&
    loudness.value <= -6 &&
    Number.isFinite(truePeak.value) &&
    truePeak.value >= -9 &&
    truePeak.value <= 0,
);
const presets = [
  { name: "Podcast", value: -16, hint: "Voz y contenido web" },
  { name: "Streaming", value: -14, hint: "Música y plataformas" },
  { name: "Broadcast", value: -23, hint: "Objetivo EBU R128" },
];
const extensions = [
  "wav",
  "flac",
  "mp3",
  "m4a",
  "aac",
  "ogg",
  "opus",
  "aif",
  "aiff",
  "mp4",
  "mov",
  "mkv",
  "webm",
];
let previewToken = 0;
let previewId = "";
let analysisToken = 0;
let analysisId = "";
let stopDrop: (() => void) | undefined;
let stopDrag: (() => void) | undefined;
let disposed = false;

function parse(value: string): number | null {
  const parts = value.trim().split(":");
  if (!value.trim() || parts.length > 3) return null;
  let seconds = 0;
  for (let i = 0; i < parts.length; i++) {
    if (!/^\d+(?:\.\d+)?$/.test(parts[i])) return null;
    const number = Number(parts[i]);
    if (
      !Number.isFinite(number) ||
      (i > 0 && number >= 60) ||
      (i < parts.length - 1 && number % 1 !== 0)
    )
      return null;
    seconds = seconds * 60 + number;
  }
  return Number.isFinite(seconds) ? seconds : null;
}
const startSeconds = computed(() => parse(start.value));
const endSeconds = computed(() => parse(end.value));
const selectionValid = computed(
  () =>
    startSeconds.value !== null &&
    endSeconds.value !== null &&
    startSeconds.value >= 0 &&
    endSeconds.value <= duration.value + 0.001 &&
    endSeconds.value - startSeconds.value >= 0.001,
);
const selectionDuration = computed(() =>
  selectionValid.value ? endSeconds.value! - startSeconds.value! : 0,
);
const durationSplitProblem = computed(() => {
  if (splitMode.value !== "duration" || !selectionValid.value) return "";
  const seconds = splitDurationSeconds.value;
  if (seconds === null || seconds < 0.001)
    return "Introduce una duración de al menos 0.001 s por segmento.";
  const intervalMs = Math.round(seconds * 1000);
  const selectionMs =
    Math.round(endSeconds.value! * 1000) - Math.round(startSeconds.value! * 1000);
  if (intervalMs >= selectionMs)
    return "La duración por segmento debe ser menor que la selección.";
  if (snapToPause.value && (!Number.isFinite(pauseWindow.value) || pauseWindow.value <= 0 || pauseWindow.value > 60))
    return "Introduce una ventana de búsqueda mayor que 0 s y de hasta 60 s.";
  return "";
});
const percent = (seconds: number) =>
  duration.value
    ? Math.max(0, Math.min(100, (seconds / duration.value) * 100))
    : 0;
const inRangeMarkers = computed(() =>
  markers.value.filter(
    (marker) =>
      marker > (startSeconds.value || 0) &&
      marker < (endSeconds.value || duration.value),
  ),
);
const manualSegments = computed<Segment[]>(() => {
  if (!selectionValid.value) return [];
  const points = [
    startSeconds.value!,
    ...inRangeMarkers.value,
    endSeconds.value!,
  ];
  return points
    .slice(0, -1)
    .map((point, i) => ({ startSeconds: point, endSeconds: points[i + 1] }));
});
const segments = computed(() =>
  splitMode.value === "manual" ? manualSegments.value : analyzedSegments.value,
);
const autoReviewed = computed(
  () => splitMode.value === "manual" || analyzedSegments.value.length > 1,
);
const formats = computed(() =>
  selected.value?.probe?.mediaKind === "video" && tool.value !== "normalize"
    ? videoFormats
    : audioFormats,
);
const splitFadeProblem = computed(() => {
  if (!Number.isFinite(splitFades.value.in) || !Number.isFinite(splitFades.value.out) ||
      splitFades.value.in < 0 || splitFades.value.out < 0)
    return "Introduce fades de entrada y salida de 0 s o más.";
  return "";
});
const splitEffectProblem = computed(() => splitFadeProblem.value || (
  splitNormalization.value !== "none" && !loudnessValid.value
    ? "Revisa los objetivos de sonoridad y true peak."
    : ""
));
const exportProblem = computed(() => {
  if (!sources.value.length) return "Importa un archivo para empezar.";
  if (!queueReady.value)
    return "Abre la aplicación de escritorio para procesar archivos.";
  if (destination.value === "custom" && !outputDir.value)
    return "Selecciona la carpeta de destino.";
  if (tool.value === "normalize") {
    if (sources.value.some((source) => !source.probe || source.error))
      return "Espera a que se verifiquen todos los archivos o retira los que tienen errores.";
    if (!loudnessValid.value)
      return "Revisa los objetivos de sonoridad y true peak.";
    return "";
  }
  if (!selected.value?.probe) return "Selecciona un archivo válido.";
  if (!selectionValid.value)
    return "Revisa el inicio y el final de la selección.";
  if (
    !Number.isFinite(fades.value.in) ||
    !Number.isFinite(fades.value.out) ||
    fades.value.in < 0 ||
    fades.value.out < 0
  )
    return "Introduce fades de entrada y salida de 0 s o más.";
  if (
    tool.value === "trim" &&
    fades.value.in + fades.value.out > selectionDuration.value
  )
    return "Los fades deben caber dentro de la selección.";
  if (tool.value === "split") {
    if (durationSplitProblem.value) return durationSplitProblem.value;
    if (splitEffectProblem.value) return splitEffectProblem.value;
    if (!validOutputNamePattern(outputNamePattern.value))
      return "Incluye {numero} en el patrón de salida y evita caracteres reservados.";
  }
  if (
    tool.value === "split" &&
    (!autoReviewed.value || segments.value.length < 2)
  )
    return splitMode.value === "manual"
      ? "Añade al menos un corte dentro de la selección."
      : "Analiza y revisa los segmentos antes de exportar.";
  return "";
});
const audioUrl = computed(() =>
  preview.value ? convertFileSrc(preview.value.audioPath) : "",
);

async function chooseFiles() {
  error.value = "";
  try {
    const paths = await open({
      multiple: true,
      filters: [{ name: t("Audio y video"), extensions }],
    });
    if (paths) await addSources(Array.isArray(paths) ? paths : [paths]);
  } catch (e) {
    error.value = String(e);
  }
}
async function addSources(paths: string[]) {
  for (const path of paths) {
    if (!sources.value.some((source) => source.path === path))
      sources.value.push({ path });
  }
  if (!selectedPath.value) selectedPath.value = sources.value[0]?.path || "";
  for (const path of paths) {
    const source = sources.value.find((source) => source.path === path);
    if (!source || source.probe) continue;
    try {
      source.probe = await invoke<MediaProbeResponse>("probe_media", {
        inputPath: path,
      });
      source.error = undefined;
    } catch (e) {
      source.error = String(e);
    }
  }
}
function removeSource(path: string) {
  sources.value = sources.value.filter((source) => source.path !== path);
  if (selectedPath.value === path)
    selectedPath.value = sources.value[0]?.path || "";
}
function stopPlayback() {
  playbackRequest++;
  playbackStarting.value = false;
  cancelAnimationFrame(animationFrame);
  audio.value?.pause();
  playing.value = false;
}
function resetPlayback() {
  stopPlayback();
  comparisonPlayer.value?.stop();
  seek(playbackSegment.value?.startSeconds ?? startSeconds.value ?? 0);
}
function seek(seconds: number) {
  cursor.value = Math.max(0, Math.min(duration.value, seconds));
  lastInspectedTime.value = cursor.value;
  if (audio.value) {
    try {
      audio.value.currentTime = cursor.value;
    } catch {
      /* Metadata is still loading. */
    }
  }
}
let playbackRequest = 0;
async function playRegion(regionStart: number, regionEnd: number, from = regionStart) {
  const element = audio.value;
  if (!element || !preview.value) return;
  comparisonPlayer.value?.pause();
  const request = ++playbackRequest;
  playbackEnd = regionEnd;
  playbackStart = regionStart;
  seek(from);
  playbackStarting.value = true;
  try {
    await element.play();
  } catch (e) {
    if (request === playbackRequest) {
      previewError.value = t("No se pudo reproducir la preescucha: {error}", { error: String(e) });
    }
  } finally {
    if (request === playbackRequest) playbackStarting.value = false;
  }
}
let animationFrame = 0;
function onAudioPlay() {
  if (audio.value?.paused !== false) return;
  playing.value = true;
  cancelAnimationFrame(animationFrame);
  const frame = () => {
    updateTime();
    if (playing.value) animationFrame = requestAnimationFrame(frame);
  };
  animationFrame = requestAnimationFrame(frame);
}
function onAudioPause() {
  if (audio.value?.paused) {
    playing.value = false;
    cancelAnimationFrame(animationFrame);
  }
}
let playbackStart = 0;
let playbackEnd = 0;
function togglePlayback() {
  if (playbackActive.value) {
    stopPlayback();
    return;
  }
  if (playbackSegment.value) {
    toggleSegmentPlayback(playbackSegment.value);
    return;
  }
  const regionStart = selectionValid.value ? startSeconds.value! : 0;
  const regionEnd = selectionValid.value ? endSeconds.value! : duration.value;
  void playRegion(
    regionStart,
    regionEnd,
    cursor.value >= regionStart && cursor.value < regionEnd - 0.01
      ? cursor.value
      : regionStart,
  );
}
function toggleSegmentPlayback(segment: Segment) {
  const isCurrent = playbackSegment.value === segment;
  if (isCurrent && playbackActive.value) {
    stopPlayback();
    return;
  }
  playbackSegment.value = segment;
  const from = isCurrent && cursor.value >= segment.startSeconds && cursor.value < segment.endSeconds
    ? cursor.value
    : segment.startSeconds;
  void playRegion(segment.startSeconds, segment.endSeconds, from);
}
function segmentElapsed(segment: Segment) {
  return Math.max(0, Math.min(cursor.value, segment.endSeconds) - segment.startSeconds);
}
function updateTime() {
  if (!audio.value) return;
  cursor.value = Math.min(duration.value, audio.value.currentTime);
  if (playing.value && cursor.value >= playbackEnd) {
    if (loop.value) seek(playbackStart);
    else {
      stopPlayback();
      seek(playbackEnd);
    }
  }
}
function ended() {
  if (loop.value) void playRegion(playbackStart, playbackEnd);
  else stopPlayback();
}
function position(event: PointerEvent) {
  const bounds = timeline.value!.getBoundingClientRect();
  return Math.max(
    0,
    Math.min(
      duration.value,
      ((event.clientX - bounds.left) / bounds.width) * duration.value,
    ),
  );
}
function dragHandle(event: PointerEvent, handle: "start" | "end") {
  event.preventDefault();
  event.stopPropagation();
  stopDrag?.();
  stopPlayback();
  const move = (event: PointerEvent) => {
    const point = position(event);
    if (handle === "start")
      start.value = time(
        Math.min(point, (endSeconds.value || duration.value) - 0.001),
      );
    else end.value = time(Math.max(point, (startSeconds.value || 0) + 0.001));
  };
  const cleanup = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", cleanup);
    window.removeEventListener("pointercancel", cleanup);
    stopDrag = undefined;
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", cleanup);
  window.addEventListener("pointercancel", cleanup);
  stopDrag = cleanup;
}
function addMarker(at = cursor.value) {
  if (
    !selectionValid.value ||
    at <= startSeconds.value! ||
    at >= endSeconds.value!
  ) {
    error.value = "El corte debe estar dentro de la selección.";
    return;
  }
  if (markers.value.some((marker) => Math.abs(marker - at) < 0.001)) return;
  markers.value = [...markers.value, Math.round(at * 1000) / 1000].sort(
    (a, b) => a - b,
  );
  markerInput.value = "";
  error.value = "";
}
function addTypedMarker() {
  const point = parse(markerInput.value);
  if (point === null) {
    error.value = "Escribe un tiempo válido: mm:ss.mmm o segundos.";
    return;
  }
  addMarker(point);
}
function resetSelection() {
  start.value = time(0);
  end.value = time(duration.value);
  seek(0);
}
function baseRequest(path: string): ProcessJobRequest {
  const isSplit = tool.value === "split";
  const usePauses = isSplit && splitMode.value === "duration" && snapToPause.value;
  const useSilence = isSplit && splitMode.value === "silence";
  const normalize = tool.value === "normalize";
  const useLoudness =
    normalize || (isSplit && splitNormalization.value !== "none");
  return {
    inputPath: path,
    outputDir: destination.value === "custom" ? outputDir.value : undefined,
    format: format.value,
    encoding: compatibleEncoding(format.value, currentEncoding.value),
    trimStart:
      tool.value === "normalize" ? undefined : String(startSeconds.value),
    trimEnd: tool.value === "normalize" ? undefined : String(endSeconds.value),
    normalize,
    loudnessTargetLufs: useLoudness ? loudness.value : -16,
    truePeakDbtp: useLoudness ? truePeak.value : -1.5,
    eq: { bass: 0, mid: 0, treble: 0 },
    splitMode: isSplit ? splitMode.value : "none",
    splitDurationSeconds:
      isSplit && splitMode.value === "duration"
        ? splitDurationSeconds.value ?? undefined
        : undefined,
    splitPauseWindowSeconds: usePauses ? pauseWindow.value : undefined,
    outputNamePattern: isSplit ? outputNamePattern.value.trim() || undefined : undefined,
    splitNormalization: isSplit ? splitNormalization.value : "none",
    manualMarkers: [],
    silence: {
      thresholdDb: !autoSilence.value && (useSilence || usePauses) ? thresholdDb.value : -35,
      minSilenceDuration: !autoSilence.value && usePauses ? snapMinSilence.value
        : !autoSilence.value && useSilence ? minSilence.value : 1.5,
      minSegmentDuration: useSilence ? minSegment.value : 20,
      automatic: (useSilence || usePauses) && autoSilence.value,
      sensitivity: silenceSensitivity.value,
    },
    fadeInDuration: normalize ? 0 : fades.value.in,
    fadeOutDuration: normalize ? 0 : fades.value.out,
  };
}

function applySplitPreset() {
  const preset = storedPresets.value.find(({ id }) => id === presetChoice.value);
  presetStatus.value = "";
  presetName.value = preset?.name ?? "";
  if (!preset) return;
  splitMode.value = preset.mode;
  splitDuration.value = time(preset.duration);
  snapToPause.value = preset.snapToPause;
  pauseWindow.value = preset.pauseWindow;
  snapMinSilence.value = preset.snapMinSilence;
  autoSilence.value = preset.autoSilence ?? false;
  silenceSensitivity.value = preset.sensitivity ?? "balanced";
  scanAutomatically.value = preset.scanAutomatically ?? false;
  thresholdDb.value = preset.thresholdDb;
  minSilence.value = preset.minSilence;
  minSegment.value = preset.minSegment;
  splitFades.value = { in: preset.fadeIn, out: preset.fadeOut };
  splitNormalization.value = preset.normalization;
  loudness.value = preset.loudness;
  truePeak.value = preset.truePeak;
  outputNamePattern.value = preset.outputNamePattern;
  const compatible = !selected.value?.probe || formats.value.includes(preset.format);
  format.value = compatible ? preset.format : "wav";
  currentEncoding.value = compatible
    ? compatibleEncoding(preset.format, preset.encoding ?? {})
    : {};
  presetStatus.value = t(
    compatible ? "Aplicado: {name}." : "Aplicado: {name}. Se usará WAV para este archivo.",
    { name: preset.name },
  );
}

function persistSplitPresets(presets: SplitPreset[]) {
  try {
    localStorage.setItem(splitPresetsKey, JSON.stringify(presets));
    storedPresets.value = presets;
    return true;
  } catch {
    presetStatus.value = "No se pudieron guardar los presets en este equipo.";
    return false;
  }
}

const matchingPreset = computed(() => storedPresets.value.find(
  ({ name }) => name.toLocaleLowerCase() === presetName.value.trim().toLocaleLowerCase(),
));

function saveSplitPreset() {
  const id = matchingPreset.value?.id ?? crypto.randomUUID();
  const preset: SplitPreset = {
    id,
    name: presetName.value.trim(),
    mode: splitMode.value,
    duration: splitDurationSeconds.value ?? 0,
    snapToPause: snapToPause.value,
    pauseWindow: pauseWindow.value,
    snapMinSilence: snapMinSilence.value,
    autoSilence: autoSilence.value,
    sensitivity: silenceSensitivity.value,
    scanAutomatically: scanAutomatically.value,
    thresholdDb: thresholdDb.value,
    minSilence: minSilence.value,
    minSegment: minSegment.value,
    fadeIn: splitFades.value.in,
    fadeOut: splitFades.value.out,
    normalization: splitNormalization.value,
    loudness: loudness.value,
    truePeak: truePeak.value,
    format: format.value,
    encoding: compatibleEncoding(format.value, currentEncoding.value),
    outputNamePattern: outputNamePattern.value.trim(),
  };
  if (!isSplitPreset(preset)) {
    presetStatus.value = "Revisa el nombre y los ajustes de Split antes de guardar.";
    return;
  }
  const next = matchingPreset.value
    ? storedPresets.value.map((item) => item.id === id ? preset : item)
    : [...storedPresets.value, preset];
  if (persistSplitPresets(next)) {
    presetChoice.value = id;
    presetStatus.value = t("Guardado: {name}.", { name: preset.name });
  }
}

function deleteSplitPreset() {
  if (persistSplitPresets(storedPresets.value.filter(({ id }) => id !== presetChoice.value))) {
    presetChoice.value = "";
    presetName.value = "";
    presetStatus.value = "Preset eliminado.";
  }
}

function splitRequest(path: string, splitSegments: Segment[]) {
  const request = baseRequest(path);
  request.splitMode = "manual";
  request.splitDurationSeconds = undefined;
  request.splitPauseWindowSeconds = undefined;
  request.silence.automatic = false;
  request.manualMarkers = splitSegments.slice(1).map(({ startSeconds }) => startSeconds);
  return request;
}

function invalidateComparison() {
  const id = comparisonId;
  const pending = comparisonPending.value;
  comparisonPlayer.value?.pause();
  comparisonId = "";
  comparison.value = undefined;
  comparisonSegment.value = undefined;
  comparisonPending.value = false;
  comparisonPercent.value = 0;
  comparisonError.value = "";
  if (pending) void invoke("cancel_job", { id }).catch(() => {});
  else if (id) void nextTick().then(() => invoke("remove_split_comparison", { id })).catch(() => {});
}

async function compareSegment(segment: Segment, index: number) {
  invalidateComparison();
  comparisonSegment.value = segment;
  comparisonIndex.value = index + 1;
  if (splitFadeProblem.value) {
    comparisonError.value = splitFadeProblem.value;
    return;
  }
  stopPlayback();
  const id = crypto.randomUUID();
  comparisonId = id;
  comparisonPending.value = true;
  void nextTick(() => {
    if (id === comparisonId) comparisonPanel.value?.scrollIntoView({ block: "nearest" });
  });
  const onUpdate = new Channel<SplitComparisonUpdate>(({ percent, originalPath }) => {
    if (id !== comparisonId) return;
    comparisonPercent.value = Math.max(comparisonPercent.value, percent);
    if (originalPath && !comparison.value) comparison.value = { originalPath };
  });
  const { inputPath, trimStart, trimEnd, eq, fadeInDuration, fadeOutDuration } = baseRequest(selectedPath.value);
  try {
    const result = await invoke<SplitComparison>("render_split_comparison", {
      request: {
        id,
        inputPath,
        trimStart,
        trimEnd,
        eq,
        fadeInDuration,
        fadeOutDuration,
        format: "wav",
        normalize: false,
      },
      segment,
      onUpdate,
    });
    if (id === comparisonId) {
      comparison.value = result;
      comparisonPercent.value = 100;
    }
  } catch (error) {
    if (id === comparisonId) comparisonError.value = String(error);
  } finally {
    if (id === comparisonId) comparisonPending.value = false;
    else await nextTick().then(() => invoke("remove_split_comparison", { id })).catch(() => {});
  }
}
async function invalidateAnalysis() {
  clearTimeout(analysisTimer);
  analysisToken++;
  analyzedSegments.value = [];
  automaticAnalysis.value = undefined;
  analysisBusy.value = false;
  if (analysisId) {
    const id = analysisId;
    analysisId = "";
    try {
      await invoke("cancel_job", { id });
    } catch {
      /* Already completed. */
    }
  }
}
async function analyze() {
  if (!isTauri() || !selectionValid.value || !selectedPath.value || tool.value !== "split") return;
  if (durationSplitProblem.value) {
    error.value = durationSplitProblem.value;
    return;
  }
  void invalidateAnalysis();
  const token = ++analysisToken;
  const id = crypto.randomUUID();
  analysisId = id;
  analysisBusy.value = true;
  error.value = "";
  try {
    const result = await invoke<SplitPlan>("analyze_split", {
      request: {
        ...baseRequest(selectedPath.value),
        id,
        splitNormalization: "none",
        outputNamePattern: undefined,
        encoding: undefined,
        loudnessTargetLufs: -16,
        truePeakDbtp: -1.5,
        fadeInDuration: 0,
        fadeOutDuration: 0,
      },
    });
    if (token !== analysisToken || disposed) return;
    analyzedSegments.value = result.segments;
    automaticAnalysis.value = result.automatic ?? undefined;
    if (result.segments.length < 2)
      error.value =
        "No se encontraron pausas para cortar. Prueba otra sensibilidad, reduce el segmento mínimo o añade cortes manuales.";
  } catch (e) {
    if (token === analysisToken && !disposed) error.value = String(e);
  } finally {
    if (token === analysisToken) {
      analysisBusy.value = false;
      analysisId = "";
    }
  }
}

function scheduleAutomaticAnalysis() {
  void invalidateAnalysis();
  if (isTauri() && tool.value === "split" && usesPauses.value && scanAutomatically.value &&
      selectionValid.value && !durationSplitProblem.value &&
      (!autoSilence.value || !levelsBusy.value)) {
    analysisTimer = setTimeout(() => void analyze(), 450);
  }
}
function editDetectedCuts() {
  const cuts = segments.value
    .slice(1)
    .map((segment) => segment.startSeconds);
  splitMode.value = "manual";
  markers.value = cuts;
}
async function chooseDestination() {
  try {
    const path = await open({ directory: true });
    if (typeof path === "string") {
      outputDir.value = path;
      destination.value = "custom";
    }
  } catch (e) {
    error.value = String(e);
  }
}
function exportJobs() {
  if (exportProblem.value) return;
  try {
    const paths =
      tool.value === "normalize"
        ? sources.value.map((source) => source.path)
        : [selectedPath.value];
    const requests = paths.map((path) => tool.value === "split"
      ? splitRequest(path, segments.value)
      : baseRequest(path));
    enqueue(requests, tool.value);
    notice.value = t("{count} tarea(s) añadida(s) a la cola.", { count: requests.length });
    showQueue.value = true;
  } catch (e) {
    error.value = String(e);
  }
}
function keydown(event: KeyboardEvent) {
  const target = event.target as HTMLElement;
  if (!props.active) return;
  if (
    event.ctrlKey ||
    event.metaKey ||
    event.altKey ||
    target.matches("input, select, textarea, button") ||
    target.isContentEditable ||
    showQueue.value
  )
    return;
  if (!selected.value?.probe || tool.value === "normalize") return;
  if (event.code === "Space") {
    event.preventDefault();
    togglePlayback();
  } else if (event.key.toLowerCase() === "i")
    start.value = time(
      Math.min(cursor.value, (endSeconds.value || duration.value) - 0.001),
    );
  else if (event.key.toLowerCase() === "o")
    end.value = time(Math.max(cursor.value, (startSeconds.value || 0) + 0.001));
  else if (
    event.key.toLowerCase() === "m" &&
    tool.value === "split" &&
    splitMode.value === "manual"
  )
    addMarker();
  else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    event.preventDefault();
    seek(
      cursor.value +
        (event.key === "ArrowLeft" ? -1 : 1) * (event.shiftKey ? 0.1 : 1),
    );
  }
}
async function cancelPreview() {
  previewToken++;
  previewBusy.value = false;
  if (previewId) {
    const id = previewId;
    previewId = "";
    try {
      await invoke("cancel_job", { id });
    } catch {
      /* Finished. */
    }
  }
}
async function loadPreview(path: string) {
  await cancelPreview();
  if (selectedPath.value !== path || disposed) return;
  const id = crypto.randomUUID();
  previewId = id;
  const token = ++previewToken;
  previewBusy.value = true;
  try {
    const result = await invoke<WaveformPreviewResponse>(
      "render_waveform_preview",
      { inputPath: path, id },
    );
    if (token !== previewToken || disposed) return;
    preview.value = result;
  } catch (e) {
    if (token === previewToken && !disposed) previewError.value = String(e);
  } finally {
    if (token === previewToken) {
      previewBusy.value = false;
      previewId = "";
    }
  }
}
watch(selectedPath, async (path) => {
  void cancelPreview();
  void cancelLevels();
  levels.value = undefined;
  levelsError.value = "";
  hoverTime.value = null;
  lastInspectedTime.value = null;
  stopPlayback();
  playbackSegment.value = undefined;
  stopDrag?.();
  void invalidateAnalysis();
  preview.value = undefined;
  previewError.value = "";
  cursor.value = 0;
  start.value = time(0);
  end.value = duration.value ? time(duration.value) : "";
  markers.value = [];
  zoomSpan.value = undefined;
  jumpTime.value = "";
  jumpInvalid.value = false;
  scrollOffset.value = 0;
  if (waveScroll.value) waveScroll.value.scrollLeft = 0;
  if (!path) {
    previewBusy.value = false;
    return;
  }
  if (tool.value === "normalize") {
    previewBusy.value = false;
    return;
  }
  void loadLevels(path);
  void loadPreview(path);
});
watch(duration, (value) => {
  end.value = value ? time(value) : "";
});
watch(
  [tool, selectedPath, start, end, splitMode, splitDuration, snapToPause, pauseWindow, snapMinSilence,
    autoSilence, scanAutomatically, silenceSensitivity, thresholdDb, minSilence, minSegment],
  scheduleAutomaticAnalysis,
);
watch([automaticAnalysis, autoSilence, snapToPause, snapMinSilence], schedulePaint);
watch(levelsBusy, (busy) => {
  if (!busy && autoSilence.value && scanAutomatically.value && usesPauses.value &&
      !analysisBusy.value && !analyzedSegments.value.length) {
    scheduleAutomaticAnalysis();
  }
});
watch([start, end, segments], () => {
  stopPlayback();
  playbackSegment.value = undefined;
});
watch(showQueue, (value) => {
  if (value) {
    stopPlayback();
    invalidateComparison();
  }
});
watch([tool, selectedPath, formats], () => {
  if (selected.value?.probe && !formats.value.includes(format.value)) format.value = formats.value[0];
  stopPlayback();
  if (tool.value !== "split") playbackSegment.value = undefined;
});
watch(
  [tool, selectedPath, start, end, segments, () => splitFades.value.in,
    () => splitFades.value.out],
  invalidateComparison,
);
watch(
  () => props.active,
  (active) => {
    if (!active) {
      stopPlayback();
      invalidateComparison();
    }
  },
);
watch(
  [
    levels,
    zoomSpan,
    thresholdDb,
    minSilence,
    tool,
    splitMode,
    duration,
    () => props.active,
    showQueue,
  ],
  async () => {
    await nextTick();
    resizeObserver?.disconnect();
    if (waveScroll.value) resizeObserver?.observe(waveScroll.value);
    schedulePaint();
  },
);
watch(tool, () => {
  if (tool.value === "normalize") {
    void cancelPreview();
    void cancelLevels();
  }
  if (
    tool.value !== "normalize" &&
    selectedPath.value &&
    !preview.value &&
    !previewBusy.value
  ) {
    void loadPreview(selectedPath.value);
    if (!levels.value && !levelsBusy.value) void loadLevels(selectedPath.value);
  }
  showQueue.value = false;
  error.value = "";
  notice.value = "";
});
onMounted(async () => {
  window.addEventListener("keydown", keydown);
  if (isTauri()) {
    const unlisten = await getCurrentWebview().onDragDropEvent(
      ({ payload }) => {
        dragging.value = payload.type === "enter" || payload.type === "over";
        if (payload.type === "drop") {
          dragging.value = false;
          void addSources(payload.paths);
        }
      },
    );
    if (disposed) unlisten();
    else stopDrop = unlisten;
  }
  await nextTick();
  resizeObserver = new ResizeObserver(schedulePaint);
  if (waveScroll.value) resizeObserver.observe(waveScroll.value);
});
onUnmounted(() => {
  disposed = true;
  invalidateComparison();
  void cancelPreview();
  void cancelLevels();
  resizeObserver?.disconnect();
  cancelAnimationFrame(paintFrame);
  stopPlayback();
  stopDrag?.();
  stopDrop?.();
  void invalidateAnalysis();
  window.removeEventListener("keydown", keydown);
});
</script>

<template>
  <div class="studio" :class="{ 'drop-active': dragging }">
    <aside class="studio-sidebar">
      <div class="sidebar-label">{{ t('ESTUDIO LOCAL') }}</div>
      <button
        v-for="item in tools"
        :key="item.id"
        :aria-label="item.name"
        :title="item.name"
        class="tool-nav"
        :class="{ active: tool === item.id && !showQueue }"
        @click="
          tool = item.id;
          showQueue = false;
        "
      >
        <component :is="item.icon" :size="20" /><span
          ><strong>{{ item.name }}</strong
          ><small>{{ t(item.hint) }}</small></span
        ><ChevronRight v-if="tool === item.id && !showQueue" :size="15" />
      </button>
      <div class="sidebar-divider" />
      <button
        class="tool-nav queue-nav"
        :aria-label="t('Exportaciones')"
        :title="t('Exportaciones')"
        :class="{ active: showQueue }"
        @click="showQueue = true"
      >
        <ListOrdered :size="20" /><span
          ><strong>{{ t('Exportaciones') }}</strong
          ><small>{{ t('Progreso e historial') }}</small></span
        ><span v-if="pendingCount" class="count">{{ pendingCount }}</span>
      </button>
      <div class="sidebar-bottom">
        <span class="status-dot" /><span
          >{{ t('Tu archivo. Tu equipo.') }}<br /><small
            >{{ t('Procesamiento local con FFmpeg') }}</small
          ></span
        >
      </div>
    </aside>

    <div class="studio-main">
      <template v-if="showQueue">
        <div class="workspace-heading">
          <div>
            <div class="eyebrow">{{ t('ESPACIO DE TRABAJO / COLA') }}</div>
            <h1>{{ t('Exportaciones') }}<span>.</span></h1>
            <p>
              {{
                pendingCount
                  ? t('{count} tarea(s) en curso o en espera.', { count: pendingCount })
                  : t('Tus resultados, en un solo lugar.')
              }}
            </p>
          </div>
          <button
            class="secondary-button"
            :disabled="
              !queue.some(
                (item) => !['waiting', 'processing'].includes(item.status),
              )
            "
            @click="clearHistory"
          >
            {{ t('Limpiar historial') }}
          </button>
        </div>
        <div v-if="notice" class="notice" role="status">
          <Check :size="16" />{{ notice
          }}<button
            class="icon-button"
            :aria-label="t('Cerrar aviso')"
            @click="notice = ''"
          >
            <X :size="15" />
          </button>
        </div>
        <div v-if="!queue.length" class="empty-queue panel">
          <Layers :size="36" />
          <h2>{{ t('Lista para tu primera exportación') }}</h2>
          <p>{{ t('Prepara un recorte, divide un archivo o normaliza tu audio.') }}</p>
          <button class="secondary-button" @click="showQueue = false">
            {{ t('Volver al estudio') }} <ArrowRight :size="16" />
          </button>
        </div>
        <div v-else class="queue-list">
          <QueueItem
            v-for="item in [...queue].reverse()"
            :key="item.id"
            :item="item"
            @cancel="cancelItem(item)"
            @retry="retryItem(item)"
            @reveal="revealOutput($event).catch((e) => (error = String(e)))"
          />
        </div>
      </template>

      <template v-else>
        <div class="workspace-heading">
          <div>
            <div class="eyebrow">
              {{ t('ESPACIO DE TRABAJO /') }} {{ tool.toUpperCase() }}
            </div>
            <h1>{{ currentTool.name }}<span>.</span></h1>
            <p>{{ t(currentTool.description) }}</p>
          </div>
          <span class="local-badge"
            ><span class="status-dot" />{{ t('ARCHIVOS LOCALES') }}</span
          >
        </div>
        <div v-if="!isTauri()" class="notice">
          {{ t('La interfaz está en modo navegador. Abre MediaToolsPro en escritorio para importar y procesar archivos.') }}
        </div>
        <div class="source-panel panel">
          <div class="section-top">
            <div>
              <span class="step-number">01</span>
              <h2>{{ t('Archivos de origen') }}</h2>
              <span v-if="sources.length" class="tag">{{
                sources.length
              }}</span>
            </div>
            <button
              class="secondary-button compact"
              :disabled="!isTauri()"
              @click="chooseFiles"
            >
              <Plus :size="15" />{{ t('Añadir archivos') }}
            </button>
          </div>
          <button
            v-if="!sources.length"
            class="import-zone"
            :disabled="!isTauri()"
            @click="chooseFiles"
          >
            <span class="upload-icon"><Upload :size="24" /></span
            ><strong>{{ t('Arrastra tus archivos aquí') }}</strong
            ><span>{{ t('o haz clic para explorar tu equipo') }}</span
            ><small>{{ t('WAV, FLAC, MP3, M4A, MP4 y más formatos') }}</small>
          </button>
          <div v-else class="source-list">
            <div
              v-for="source in sources"
              :key="source.path"
              class="source-row"
              :class="{ selected: selectedPath === source.path }"
            >
              <button class="source-select" @click="selectedPath = source.path">
                <FileAudio :size="19" /><span
                  ><strong :title="source.path">{{
                    basename(source.path)
                  }}</strong
                  ><small :class="{ 'error-text': source.error }">{{
                    tMediaMessage(source.error || '') ||
                    (source.probe
                      ? `${source.probe.mediaKind.toUpperCase()} · ${source.probe.durationLabel}`
                      : t('Leyendo archivo…'))
                  }}</small></span
                ><Check
                  v-if="selectedPath === source.path"
                  :size="16"
                /></button
              ><button
                class="icon-button"
                :aria-label="t('Retirar archivo')"
                :title="t('Retirar archivo')"
                @click="removeSource(source.path)"
              >
                <X :size="16" />
              </button>
            </div>
          </div>
          <p v-if="sources.length > 1" class="panel-footnote">
            {{
              tool === "normalize"
                ? t('Se normalizará cada archivo por separado con el mismo objetivo.')
                : t('Editando el archivo seleccionado. Cada exportación usa sus propios cortes.')
            }}
          </p>
        </div>

        <div class="editor-grid">
          <section class="editor-panel panel">
            <div class="section-top">
              <div>
                <span class="step-number">02</span>
                <h2>
                  {{
                    tool === "normalize"
                      ? t('Objetivo de sonoridad')
                      : t('Edición y preescucha')
                  }}
                </h2>
              </div>
              <span class="tag">{{
                  t(tool === "normalize" ? t('DOS PASADAS') : t('PRECISIÓN · 1 ms'))
              }}</span>
              <button v-if="tool === 'split'" class="secondary-button compact"
                @click="splitOptions?.scrollIntoView({ block: 'start' })">
                {{ t("Configurar cortes") }}
              </button>
            </div>
            <template v-if="tool === 'normalize'">
              <div class="loudness-intro">
                <span class="loudness-symbol"><Activity :size="30" /></span>
                <div>
                  <h3>{{ t('El mismo objetivo. Cada archivo medido.') }}</h3>
                  <p>
                    {{ t('Primero analizamos la sonoridad; después aplicamos el ajuste. El límite de true peak se usa en la normalización.') }}
                  </p>
                </div>
              </div>
            </template>
            <template v-else>
              <div class="timeline-toolbar">
                <div class="transport">
                  <button
                    class="play-button"
                    :disabled="!preview || (!selectionValid && !playbackActive)"
                    :aria-label="t(playbackActive ? 'Pausar' : 'Reproducir selección')"
                    :title="t(playbackActive ? 'Pausar' : 'Reproducir selección')"
                    @click="togglePlayback"
                  >
                    <Pause v-if="playbackActive" :size="17" /><Play
                      v-else
                      :size="17"
                    /></button>
                  <button class="icon-button" :disabled="!preview && !comparison" :aria-label="t('Detener reproducción')"
                    :title="t('Detener reproducción')" @click="resetPlayback"><Square :size="14" /></button>
                  <span class="timecode">{{ time(cursor) }}</span
                  ><span class="time-total">/ {{ time(duration) }}</span>
                </div>
                <div class="transport">
                  <button
                    class="icon-button"
                    :class="{ enabled: loop }"
                    :aria-pressed="loop"
                    :title="t('Repetir selección')"
                    :aria-label="t('Repetir selección')"
                    @click="loop = !loop"
                  >
                    <Repeat2 :size="17" /></button
                  ><span class="toolbar-divider" /><button
                    class="icon-button"
                    :aria-label="t('Reducir zoom')"
                    :title="t('Reducir zoom')"
                    :disabled="zoomIndex <= 0"
                    @click="changeZoom(zoomSpans[zoomIndex - 1])"
                  >
                    <Minus :size="16" /></button
                  ><select
                    v-model="activeZoom"
                    class="wave-zoom"
                    :aria-label="t('Duración visible')"
                    :title="t('Duración visible')"
                    :disabled="!levels || zoomSpans.length < 2"
                  >
                    <option v-for="span in zoomSpans" :key="span" :value="span">
                      {{ zoomLabel(span) }}
                    </option>
                  </select
                  ><button
                    class="icon-button"
                    :aria-label="t('Aumentar zoom')"
                    :title="t('Aumentar zoom')"
                    :disabled="!levels || zoomIndex >= zoomSpans.length - 1"
                    @click="changeZoom(zoomSpans[zoomIndex + 1])"
                  >
                    <Plus :size="16" />
                  </button>
                </div>
              </div>
              <div v-if="duration" class="wave-navigation">
                <div class="wave-range">
                  <span>{{ t('Intervalo visible') }}</span>
                  <span class="timecode">
                    {{ time(visibleRange.startSeconds) }} – {{ time(visibleRange.endSeconds) }}
                  </span>
                </div>
                <form class="wave-jump" @submit.prevent="jumpToTime">
                  <label for="wave-jump-time">{{ t('Ir al tiempo') }}</label>
                  <input
                    id="wave-jump-time"
                    v-model="jumpTime"
                    type="text"
                    :placeholder="duration >= 3600 ? '00:00:00.000' : '00:00.000'"
                    autocomplete="off"
                    :aria-invalid="jumpInvalid"
                    :aria-describedby="jumpInvalid ? 'wave-jump-error' : undefined"
                    @input="jumpInvalid = false"
                  />
                  <button type="submit" :disabled="!jumpTime.trim()">{{ t('Ir') }}</button>
                </form>
                <p v-if="jumpInvalid" id="wave-jump-error" class="wave-jump-error" role="status">
                  {{ t('Escribe un tiempo dentro del archivo.') }}
                </p>
              </div>
              <div
                ref="waveScroll"
                class="wave-scroll"
                @scroll.passive="waveScrolled"
              >
                <div
                  ref="timeline"
                  class="wave-timeline"
                  :style="{ width: trackWidth ? `${trackWidth}px` : '100%' }"
                  @pointerdown="duration && seek(position($event))"
                  @pointermove="
                    duration &&
                    (hoverTime = lastInspectedTime = position($event))
                  "
                  @pointerleave="hoverTime = null"
                  @dblclick="
                    tool === 'split' &&
                    splitMode === 'manual' &&
                    addMarker(cursor)
                  "
                >
                  <div class="wave-ruler">
                    <span
                      v-for="tick in rulerTicks"
                      :key="tick.seconds"
                      :class="{ 'at-start': tick.seconds === 0, 'at-end': tick.seconds === duration }"
                      :style="{ left: `${percent(tick.seconds)}%` }"
                      >{{ tick.label }}</span
                    >
                  </div>
                  <canvas
                    v-if="levels"
                    ref="waveCanvas"
                    class="wave-canvas"
                    :aria-label="t('Forma de onda detallada con niveles medidos en dBFS')"
                  />
                  <div v-else class="wave-placeholder">
                    <LoaderCircle
                      v-if="levelsBusy"
                      class="spin"
                      :size="24"
                    /><AudioLines v-else :size="32" /><span>{{
                      levelsBusy
                        ? t('Midiendo la forma de onda…')
                        : sources.length
                          ? t('La forma de onda aparecerá aquí')
                          : t('Importa un archivo para ver su forma de onda')
                    }}</span>
                  </div>
                  <div
                    v-if="levels && hoverTime !== null"
                    class="inspection-line"
                    :style="{ left: `${percent(inspectionTime)}%` }"
                  />
                  <div
                    v-if="inspectedLevel && hoverTime !== null"
                    class="wave-tooltip"
                    :style="{ left: `${tooltipLeft}px` }"
                  >
                    {{ time(inspectionTime) }} ·
                    {{ dbLabel(inspectedLevel.peakDb) }} dBFS
                  </div>
                  <template v-if="duration && selectionValid"
                    ><div
                      class="wave-shade"
                      :style="{ width: `${percent(startSeconds!)}%` }"
                    />
                    <div
                      class="wave-shade right"
                      :style="{ width: `${100 - percent(endSeconds!)}%` }"
                    />
                    <div
                      class="wave-selection"
                      :style="{
                        left: `${percent(startSeconds!)}%`,
                        width: `${percent(endSeconds! - startSeconds!)}%`,
                      }"
                    />
                    <button
                      class="trim-handle start"
                      :style="{ left: `${percent(startSeconds!)}%` }"
                      :aria-label="t('Inicio de selección')"
                      :title="t('Arrastrar inicio de selección')"
                      @pointerdown="dragHandle($event, 'start')"
                    >
                      <span /></button
                    ><button
                      class="trim-handle end"
                      :style="{ left: `${percent(endSeconds!)}%` }"
                      :aria-label="t('Final de selección')"
                      :title="t('Arrastrar final de selección')"
                      @pointerdown="dragHandle($event, 'end')"
                    >
                      <span />
                    </button>
                  </template>
                  <template v-if="tool === 'split'"
                    ><button
                      v-for="(point, i) in splitMode === 'manual'
                        ? markers
                        : analyzedSegments
                            .slice(1)
                            .map((segment) => segment.startSeconds)"
                      :key="point"
                      class="cut-marker"
                      :style="{ left: `${percent(point)}%` }"
                      :title="t('Corte {index}: {time}', { index: i + 1, time: time(point) })"
                      :aria-label="t('Ir al corte {index}', { index: i + 1 })"
                      @pointerdown.stop="seek(point)"
                    >
                      {{ i + 1 }}
                    </button></template
                  >
                  <div
                    v-if="duration"
                    class="playhead"
                    :style="{ left: `${percent(cursor)}%` }"
                  />
                </div>
              </div>
              <div v-if="levelsError" class="inline-error" role="status">
                {{ tMediaMessage(levelsError) }}
              </div>
              <div
                v-if="
                  tool === 'split' && splitMode === 'silence' && inspectedLevel &&
                  (!autoSilence || automaticAnalysis)
                "
                class="signal-inspector"
              >
                <div class="signal-reading">
                  <span class="signal-position"
                    ><Activity :size="14" />{{ time(inspectionTime) }}</span
                  ><span
                    >{{ t('Pico') }}
                    <strong
                      >{{ dbLabel(inspectedLevel.peakDb) }}
                      <small>dBFS</small></strong
                    ></span
                  ><span
                    >RMS
                    <strong
                      >{{ dbLabel(inspectedLevel.rmsDb) }}
                      <small>dBFS</small></strong
                    ></span
                  ><span :class="{ 'quiet-reading': inspectedSilence }">{{
                    inspectedSilence
                      ? t('Pausa ≈ {duration} s', { duration: silenceDuration.toFixed(2) })
                      : t(autoSilence ? 'Fuera de una pausa detectada' : 'Por encima del umbral')
                  }}</span>
                </div>
                <div v-if="!autoSilence" class="signal-actions">
                  <button
                    class="text-button"
                    :disabled="suggestedThreshold === null"
                    @click="thresholdDb = suggestedThreshold!"
                  >
                    {{ t('Usar') }} {{ suggestedThreshold }} {{ t('dB de umbral') }}
                    <ArrowRight :size="12" /></button
                  ><button
                    v-if="inspectedSilence"
                    class="text-button"
                    @click="
                      minSilence = Math.max(
                        0.05,
                        Math.min(
                          60,
                          Math.floor(silenceDuration * 0.8 * 100) / 100,
                        ),
                      )
                    "
                  >
                    {{ t('Usar pausa mínima de') }}
                    {{
                      Math.max(
                        0.05,
                        Math.min(
                          60,
                          Math.floor(silenceDuration * 0.8 * 100) / 100,
                        ),
                      ).toFixed(2)
                    }}
                    s <ArrowRight :size="12" />
                  </button>
                </div>
                <p v-if="!autoSilence">
                  {{ t('Mide una pausa para orientar el umbral. Mueve el puntero para medir; haz clic para fijar la posición. Ventanas de 20 ms, escala en dBFS. El umbral sugerido deja 3 dB sobre el pico medido; el autosplit confirma las pausas al analizar.') }}
                </p>
              </div>
              <div v-if="previewError" class="inline-error" role="status">
                {{ tMediaMessage(previewError) }} {{ t('La exportación sigue disponible si el archivo es válido.') }}
              </div>
              <audio
                v-if="audioUrl"
                ref="audio"
                :src="audioUrl"
                preload="metadata"
                @playing="onAudioPlay"
                @timeupdate="updateTime"
                @ended="ended"
                @pause="onAudioPause"
                @error="
                  stopPlayback();
                  previewError = 'No se pudo cargar el audio de preescucha.'
                "
              />
              <div class="selection-fields">
                <label
                  >{{ t('Inicio') }}
                  <input
                    v-model="start"
                    class="time-input"
                    :disabled="!duration"
                    :aria-label="t('Tiempo de inicio')"
                    @change="stopPlayback"
                  /><button
                    class="text-button"
                    :disabled="!duration"
                    @click="
                      start = time(
                        Math.min(cursor, (endSeconds || duration) - 0.001),
                      )
                    "
                  >
                    {{ t('Usar cursor') }} <kbd>I</kbd>
                  </button></label
                ><ChevronRight :size="18" class="selection-arrow" /><label
                  >{{ t('Final') }}
                  <input
                    v-model="end"
                    class="time-input"
                    :disabled="!duration"
                    :aria-label="t('Tiempo final')"
                  /><button
                    class="text-button"
                    :disabled="!duration"
                    @click="
                      end = time(Math.max(cursor, (startSeconds || 0) + 0.001))
                    "
                  >
                    {{ t('Usar cursor') }} <kbd>O</kbd>
                  </button></label
                >
                <div class="duration-info">
                  <small>{{ t('SELECCIÓN') }}</small
                  ><strong>{{ time(selectionDuration) }}</strong
                  ><button
                    class="text-button"
                    :disabled="!duration"
                    @click="resetSelection"
                  >
                    <RotateCcw :size="12" />{{ t('Restablecer') }}
                  </button>
                </div>
              </div>
              <div class="editor-hint">
                <span>{{ t('Segundos o mm:ss.mmm') }}</span
                ><span
                  ><kbd>{{ t('Espacio') }}</kbd> {{ t('reproducir ·') }} <kbd>←</kbd><kbd>→</kbd> 1 s
                  · Shift 0.1 s</span
                >
              </div>
              <template v-if="tool === 'split'">
                <div ref="splitOptions" class="split-section">
                  <div class="fields-row split-preset-picker">
                    <label>
                      {{ t('Preset de Split') }}
                      <select v-model="presetChoice" :aria-label="t('Preset de Split')" @change="applySplitPreset">
                        <option value="">{{ t('Elegir preset guardado') }}</option>
                        <option v-for="preset in storedPresets" :key="preset.id" :value="preset.id">
                          {{ preset.name }}
                        </option>
                      </select>
                    </label>
                    <button
                      v-if="presetChoice"
                      class="text-button"
                      type="button"
                      @click="deleteSplitPreset"
                    >
                      {{ t('Eliminar preset') }}
                    </button>
                  </div>
                  <div class="split-mode-tabs" :aria-label="t('Método de división')">
                    <button
                      v-for="(mode, key) in splitModes"
                      :key="key"
                      :class="{ active: splitMode === key }"
                      :aria-pressed="splitMode === key"
                      @click="splitMode = key"
                    >
                      {{ t(mode.label) }}
                    </button>
                  </div>
                  <p class="split-help">
                    {{ t(splitModes[splitMode].help) }}
                  </p>
                  <div v-if="splitMode === 'duration'" class="fields-row split-parameters">
                    <label>
                      {{ t('Duración por segmento') }}
                      <input
                        v-model="splitDuration"
                        class="time-input"
                        placeholder="03:00.000"
                        @keydown.enter="analyze"
                      />
                      <small>{{ t('Segundos o mm:ss.mmm') }}</small>
                    </label>
                  </div>
                  <template v-if="splitMode === 'duration'">
                    <label class="split-snap-toggle">
                      <input v-model="snapToPause" type="checkbox" />
                      {{ t('Buscar una pausa cerca de cada corte') }}
                    </label>
                    <div v-if="snapToPause" class="fields-row split-parameters">
                      <label>
                        {{ t('Ventana ±') }}
                        <div class="unit-input">
                          <input v-model.number="pauseWindow" type="number" min="0.001" max="60" step="0.1" />
                          <span>s</span>
                        </div>
                      </label>
                      <label v-if="!autoSilence">
                        {{ t('Pausa mínima') }}
                        <div class="unit-input">
                          <input v-model.number="snapMinSilence" type="number" min="0.05" max="60" step="0.05" />
                          <span>s</span>
                        </div>
                      </label>
                      <label v-if="!autoSilence">
                        {{ t('Umbral') }}
                        <div class="unit-input">
                          <input v-model.number="thresholdDb" type="number" min="-80" max="-5" step="1" />
                          <span>dB</span>
                        </div>
                      </label>
                    </div>
                  </template>
                  <template v-if="usesPauses">
                    <div class="fields-row split-parameters">
                      <label>
                        {{ t("Detección de pausas") }}
                        <select v-model="autoSilence">
                          <option :value="true">{{ t("Automática") }}</option>
                          <option :value="false">{{ t("Manual") }}</option>
                        </select>
                      </label>
                      <label v-if="autoSilence">
                        {{ t("Sensibilidad") }}
                        <select v-model="silenceSensitivity">
                          <option value="conservative">{{ t("Solo pausas claras") }}</option>
                          <option value="balanced">{{ t("Equilibrada") }}</option>
                          <option value="sensitive">{{ t("Incluye pausas breves") }}</option>
                        </select>
                      </label>
                    </div>
                    <label class="split-snap-toggle">
                      <input v-model="scanAutomatically" type="checkbox" />
                      {{ t("Analizar al abrir un archivo o cambiar los ajustes") }}
                    </label>
                  </template>
                  <div
                    v-if="splitMode === 'silence'"
                    class="fields-row split-parameters"
                  >
                    <label v-if="!autoSilence"
                      >{{ t('Umbral') }}
                      <div class="unit-input">
                        <input
                          v-model.number="thresholdDb"
                          type="number"
                          min="-80"
                          max="-5"
                          step="1"
                        /><span>dB</span>
                      </div></label
                    ><label v-if="!autoSilence"
                      >{{ t('Pausa mínima') }}
                      <div class="unit-input">
                        <input
                          v-model.number="minSilence"
                          type="number"
                          min="0.05"
                          max="60"
                          step="0.1"
                        /><span>s</span>
                      </div></label
                    ><label
                      >{{ t('Segmento mínimo') }}
                      <div class="unit-input">
                        <input
                          v-model.number="minSegment"
                          type="number"
                          min="0.001"
                          step="1"
                        /><span>s</span>
                      </div></label
                    >
                  </div>
                  <div v-if="splitMode === 'manual'" class="manual-tools">
                    <button
                      class="secondary-button compact"
                      :disabled="!selectionValid"
                      @click="addMarker()"
                    >
                      <Plus :size="15" />{{ t('Corte en cursor') }} <kbd>M</kbd>
                    </button>
                    <div class="marker-input">
                      <input
                        v-model="markerInput"
                        placeholder="00:30.000"
                        :aria-label="t('Tiempo de nuevo corte')"
                        @keydown.enter="addTypedMarker"
                      /><button
                        class="icon-button"
                        :disabled="!selectionValid || !markerInput"
                        :aria-label="t('Añadir corte con tiempo exacto')"
                        @click="addTypedMarker"
                      >
                        <Plus :size="16" />
                      </button>
                    </div>
                  </div>
                  <button
                    v-else
                    class="secondary-button analyze-button"
                    :disabled="!selectionValid || analysisBusy"
                    @click="analyze"
                  >
                    <LoaderCircle
                      v-if="analysisBusy"
                      :size="16"
                      class="spin"
                    /><Activity v-else :size="16" />{{
                      analysisBusy
                        ? t('Analizando archivo…')
                        : t(splitModes[splitMode].action)
                    }}
                  </button>
                  <p v-if="automaticAnalysis" class="split-help" role="status">
                    {{ t("Pausas encontradas: {count} · Umbral: {threshold} dBFS · Pausa mínima: {minimum} s", {
                      count: automaticAnalysis.pauseCount,
                      threshold: automaticAnalysis.thresholdDb,
                      minimum: automaticAnalysis.minSilenceDuration,
                    }) }}
                  </p>
                  <button
                    v-if="analysisBusy"
                    class="text-button"
                    @click="invalidateAnalysis"
                  >
                    {{ t('Cancelar análisis') }}
                  </button>
                  <div
                    v-if="splitMode === 'manual' && markers.length"
                    class="marker-chips"
                  >
                    <button
                      v-for="point in markers"
                      :key="point"
                      :class="{ muted: !inRangeMarkers.includes(point) }"
                      :title="
                        inRangeMarkers.includes(point)
                          ? t('Quitar corte')
                          : t('Fuera de la selección · quitar corte')
                      "
                      @click="
                        markers = markers.filter((marker) => marker !== point)
                      "
                    >
                      {{ time(point) }}<X :size="12" />
                    </button>
                  </div>
                </div>
                <div
                  v-if="segments.length"
                  class="segment-review"
                >
                  <div class="section-top">
                    <h3>{{ segments.length }} {{ t('segmento(s)') }}</h3>
                    <button
                      v-if="splitMode !== 'manual' && segments.length > 1"
                      class="text-button"
                      @click="editDetectedCuts"
                    >
                      {{ t('Ajustar manualmente') }}
                    </button>
                  </div>
                  <div class="segment-list">
                    <div
                      v-for="(segment, i) in segments"
                      :key="segment.startSeconds"
                      class="segment-row"
                      :class="{ 'segment-current': playbackSegment === segment }"
                    >
                      <span class="segment-number">{{
                        String(i + 1).padStart(2, "0")
                      }}</span
                      ><span
                        >{{ time(segment.startSeconds) }}
                        <span class="muted">→</span>
                        {{ time(segment.endSeconds) }}</span
                      ><small>{{
                        time(segment.endSeconds - segment.startSeconds)
                      }}</small
                      ><button
                        class="icon-button"
                        :disabled="!preview"
                        :aria-label="t(playbackSegment === segment && playbackActive ? 'Pausar segmento {index}' : 'Escuchar segmento {index}', { index: i + 1 })"
                        :title="t(playbackSegment === segment && playbackActive ? 'Pausar' : 'Escuchar segmento')"
                        @click="toggleSegmentPlayback(segment)"
                      >
                        <Pause v-if="playbackSegment === segment && playbackActive" :size="14" />
                        <Play v-else :size="14" />
                      </button>
                      <button
                        class="comparison-button"
                        :disabled="!isTauri()"
                        :aria-label="t('Comparar segmento {index}', { index: i + 1 })"
                        :aria-pressed="comparisonSegment === segment"
                        @click="compareSegment(segment, i)"
                      >
                        A/B
                      </button>
                      <div v-if="playbackSegment === segment" class="segment-playback">
                        <progress :value="segmentElapsed(segment)" :max="segment.endSeconds - segment.startSeconds"
                          :aria-label="t('Avance del segmento {index}', { index: i + 1 })" />
                        <span>{{ time(segmentElapsed(segment)) }} / {{ time(segment.endSeconds - segment.startSeconds) }}</span>
                      </div>
                    </div>
                  </div>
                  <div v-if="comparisonSegment" ref="comparisonPanel" class="segment-comparison">
                    <div class="comparison-title">
                      <h3>{{ t('Preescucha A/B · segmento') }} {{ comparisonIndex }}</h3>
                      <button class="icon-button" :aria-label="t('Cerrar comparación')" @click="invalidateComparison">
                        <X :size="14" />
                      </button>
                    </div>
                    <div v-if="comparisonPending" class="comparison-render-progress">
                      <div class="comparison-status">
                        <p role="status">{{ t(comparison ? 'Preparando la versión B…' : 'Preparando la versión A…') }}</p>
                        <span class="timecode">{{ comparisonPercent }}%</span>
                      </div>
                      <div class="progress-track" role="progressbar" :aria-label="t('Progreso de la preescucha A/B')"
                        aria-valuemin="0" aria-valuemax="100" :aria-valuenow="comparisonPercent">
                        <span :style="{ width: `${comparisonPercent}%` }" />
                      </div>
                    </div>
                    <p v-if="comparisonError" class="comparison-status error-text" role="alert">{{ tMediaMessage(comparisonError) }}</p>
                    <SplitComparisonPlayer v-if="comparison" ref="comparisonPlayer"
                      :comparison="comparison"
                      :duration="comparisonSegment.endSeconds - comparisonSegment.startSeconds"
                      @play="stopPlayback" @error="comparisonError = $event" />
                  </div>
                </div>
              </template>
            </template>
            <div v-if="tool !== 'normalize'" class="fade-section">
              <div class="fields-row">
                <label>
                  {{ t('Fade de entrada') }}
                  <div class="unit-input">
                    <input
                      v-model.number="fades.in"
                      type="number"
                      min="0"
                      :max="tool === 'trim' ? selectionDuration / 2 : undefined"
                      step="0.001"
                    /><span>s</span>
                  </div>
                </label>
                <label>
                  {{ t('Fade de salida') }}
                  <div class="unit-input">
                    <input
                      v-model.number="fades.out"
                      type="number"
                      min="0"
                      :max="tool === 'trim' ? selectionDuration / 2 : undefined"
                      step="0.001"
                    /><span>s</span>
                  </div>
                </label>
              </div>
              <p class="panel-footnote">
                {{
                  tool === "split"
                    ? t('Los fades se aplican al audio de cada segmento. Cada fade dura como máximo la mitad del segmento. Usa 0 para desactivarlo.')
                    : t('Suaviza los bordes del recorte. Usa 0 para conservar el corte directo.')
                }}
              </p>
            </div>
            <div v-if="tool === 'split'" class="split-section">
              <div class="fields-row">
                <label>
                  {{ t('Normalización de sonoridad') }}
                  <select v-model="splitNormalization">
                    <option
                      v-for="(option, key) in splitNormalizationOptions"
                      :key="key"
                      :value="key"
                    >
                      {{ t(option.label) }}
                    </option>
                  </select>
                </label>
              </div>
              <p v-if="splitNormalization !== 'none'" class="split-help normalization-help">
                {{ t(splitNormalizationOptions[splitNormalization].help) }}
              </p>
            </div>
            <template
              v-if="tool === 'normalize' || (tool === 'split' && splitNormalization !== 'none')"
            >
              <div class="preset-grid">
                <button
                  v-for="preset in presets"
                  :key="preset.value"
                  class="preset-card"
                  :class="{ selected: loudness === preset.value }"
                  :aria-pressed="loudness === preset.value"
                  @click="loudness = preset.value"
                >
                  <span
                    >{{ preset.name
                    }}<Check
                      v-if="loudness === preset.value"
                      :size="15" /></span
                  ><strong>{{ preset.value }} <small>LUFS</small></strong>
                  <p>{{ t(preset.hint) }}</p>
                </button>
              </div>
              <div class="fields-row loudness-fields">
                <label
                  >{{ t('Sonoridad integrada') }}
                  <div class="unit-input">
                    <input
                      v-model.number="loudness"
                      type="number"
                      min="-30"
                      max="-6"
                      step="0.5"
                    /><span>LUFS</span>
                  </div></label
                ><label
                  >{{ t('Techo de true peak') }}
                  <div class="unit-input">
                    <input
                      v-model.number="truePeak"
                      type="number"
                      min="-9"
                      max="0"
                      step="0.1"
                    /><span>dBTP</span>
                  </div></label
                >
              </div>
              <div v-if="tool === 'normalize'" class="method-note">
                <Layers :size="18" />
                <p>
                  {{ t('Cada archivo se mide por separado con un rango objetivo de 11 LU. La salida predeterminada es de 48 kHz.') }}
                </p>
              </div>
            </template>
          </section>

          <aside class="export-panel panel">
            <div class="section-top">
              <div>
                <span class="step-number">03</span>
                <h2>{{ t('Exportación') }}</h2>
              </div>
            </div>
            <div class="export-body">
              <label
                >{{ t('Formato de salida') }}<select v-model="format">
                  <option v-for="value in formats" :key="value" :value="value">
                    {{ value.toUpperCase()
                    }}
                  </option>
                </select></label
              >
              <ExportOptions v-model="currentEncoding" :format="format" />
              <label v-if="tool === 'split'">
                {{ t('Patrón de nombres de salida') }}
                <input v-model="outputNamePattern" placeholder="{archivo}_part{numero}" maxlength="100" />
                <small>{{ t('Usa') }} <code>{numero}</code> {{ t('para el índice y') }} <code>{archivo}</code> {{ t('para el nombre original.') }}</small>
              </label>
              <label
                >{{ t('Destino') }}<select v-model="destination">
                  <option value="source">{{ t('Junto al archivo original') }}</option>
                  <option value="custom">{{ t('Carpeta personalizada') }}</option>
                </select></label
              ><button
                v-if="destination === 'custom'"
                class="folder-picker"
                @click="chooseDestination"
              >
                <FolderOpen :size="17" /><span :title="outputDir">{{
                  outputDir || t('Seleccionar carpeta')
                }}</span>
              </button>
              <div class="export-summary">
                <div>
                  <span>{{
                    tool === "normalize" ? t('Archivos') : t('Archivo seleccionado')
                  }}</span
                  ><strong>{{
                    tool === "normalize"
                      ? sources.length
                      : selected?.probe
                        ? "1"
                        : "—"
                  }}</strong>
                </div>
                <div>
                  <span>{{
                    tool === "normalize" ? t('Objetivo') : t('Duración')
                  }}</span
                  ><strong>{{
                    tool === "normalize"
                      ? `${loudness} LUFS`
                      : time(selectionDuration)
                  }}</strong>
                </div>
                <div v-if="tool === 'split'">
                  <span>{{ t('Archivos de salida') }}</span
                  ><strong>{{
                    autoReviewed && segments.length > 1
                      ? segments.length
                      : t('Por revisar')
                  }}</strong>
                </div>
                <div v-if="tool === 'normalize'">
                  <span>True peak</span><strong>{{ truePeak }} dBTP</strong>
                </div>
              </div>
              <div class="original-note">
                <Check :size="16" />
                <p>
                  {{ t('Los originales se conservan. Cada exportación recibe un nombre disponible.') }}
                </p>
              </div>
              <button
                class="primary-button"
                :disabled="!!exportProblem || analysisBusy"
                @click="exportJobs"
              >
                {{
                  tool === "split"
                    ? t('Exportar segmentos')
                    : tool === "normalize"
                      ? t('Normalizar archivos')
                      : t('Exportar recorte')
                }}<ArrowRight :size="17" />
              </button>
              <div v-if="tool === 'split'" class="split-preset-save">
                <div class="fields-row">
                  <label>
                    {{ t('Nombre del preset') }}
                    <input v-model="presetName" maxlength="80" @keydown.enter="saveSplitPreset" />
                  </label>
                </div>
                <button class="secondary-button compact" :disabled="!presetName.trim()" @click="saveSplitPreset">
                  {{ matchingPreset ? t('Actualizar preset') : t('Guardar preset') }}
                </button>
                <p v-if="presetStatus" class="comparison-status" role="status">{{ tMediaMessage(presetStatus) }}</p>
              </div>
              <p class="export-validation" role="status">
                {{ t(exportProblem) || t('Todo listo para exportar.') }}
              </p>
            </div>
          </aside>
        </div>
      </template>
      <div v-if="error" class="error-banner" role="alert">
        <span>{{ tMediaMessage(error) }}</span
        ><button
          class="icon-button"
          :aria-label="t('Cerrar error')"
          @click="error = ''"
        >
          <X :size="16" />
        </button>
      </div>
    </div>
    <div v-if="dragging" class="drop-overlay">
      <Upload :size="38" />
      <h2>{{ t('Suelta tus archivos para importarlos') }}</h2>
    </div>
  </div>
</template>
