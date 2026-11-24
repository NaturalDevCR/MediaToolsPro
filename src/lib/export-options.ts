import type { ExportEncoding } from "../types/jobs";

type Field = {
  key: keyof ExportEncoding;
  label: string;
  fallback: string;
  options: ReadonlyArray<{ value: string | number; label: string }>;
};
const numbers = (values: number[], label: (value: number) => string) =>
  values.map((value) => ({ value, label: label(value) }));
const audioBitrates: Record<string, number[]> = {
  mp3: [96, 128, 160, 192, 256, 320],
  aac: [96, 128, 160, 192, 256],
  opus: [64, 96, 128, 160, 192, 256],
};
const videoFormats = ["mp4", "mkv", "webm", "mov"];

export function encodingFields(format: string): Field[] {
  const video = videoFormats.includes(format);
  const codec = ["mp4", "mkv", "mov", "m4a"].includes(format) ? "aac"
    : format === "webm" ? "opus" : format;
  const pcm = format === "wav" || format === "aiff";
  const rates = codec === "opus" ? [24000, 48000]
    : pcm || format === "flac" ? [44100, 48000, 96000] : [44100, 48000];
  const fields: Field[] = [
    { key: "sampleRate", label: codec === "opus" ? "Frecuencia del codificador" : "Frecuencia de muestreo",
      fallback: "Automática", options: numbers(rates, (rate) => `${rate / 1000} kHz`) },
    { key: "channels", label: "Canales de audio", fallback: "Automáticos",
      options: [{ value: 1, label: "Mono" }, { value: 2, label: "Estéreo" }] },
  ];
  const bitrate = audioBitrates[codec];
  if (bitrate) fields.push({ key: "audioBitrateKbps", label: "Tasa de bits de audio", fallback: "Predeterminada",
    options: numbers(bitrate, (value) => `${value} kb/s`) });
  if (pcm) fields.push({ key: "pcmBitDepth", label: "Profundidad de bits", fallback: "Predeterminada",
    options: numbers([16, 24, 32], (value) => `${value} bits`) });
  if (format === "ogg") fields.push({ key: "vorbisQuality", label: "Calidad Vorbis", fallback: "Predeterminada",
    options: numbers([2, 4, 6, 8, 10], String) });
  if (format === "flac") fields.push({ key: "flacCompression", label: "Compresión FLAC", fallback: "Predeterminada",
    options: numbers([0, 5, 8, 12], String) });
  if (video) fields.push({ key: "videoQuality", label: "Calidad de video", fallback: "Predeterminada",
    options: [{ value: "high", label: "Alta" }, { value: "balanced", label: "Equilibrada" }, { value: "compact", label: "Compacta" }] });
  return fields;
}

export function compatibleEncoding(format: string, settings: ExportEncoding): ExportEncoding {
  const compatible: ExportEncoding = {};
  for (const { key, options } of encodingFields(format)) {
    if (options.some(({ value }) => value === settings[key])) {
      Object.assign(compatible, { [key]: settings[key] });
    }
  }
  return compatible;
}
