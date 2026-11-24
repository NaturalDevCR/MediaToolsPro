import type { WaveformLevelsResponse } from "../types/jobs";

export interface QuietRegion {
  startSeconds: number;
  endSeconds: number;
}
export function quietRegions(
  levels: WaveformLevelsResponse,
  threshold: number,
  duration: number,
): QuietRegion[] {
  if (!Number.isFinite(threshold)) return [];
  const regions: QuietRegion[] = [];
  let start = -1;
  for (let i = 0; i <= levels.peaksDb.length; i++) {
    const quiet = i < levels.peaksDb.length && levels.peaksDb[i] < threshold;
    if (quiet && start < 0) start = i;
    else if (!quiet && start >= 0) {
      regions.push({
        startSeconds: start * levels.intervalSeconds,
        endSeconds: Math.min(duration, i * levels.intervalSeconds),
      });
      start = -1;
    }
  }
  return regions;
}
export function inspectLevel(levels: WaveformLevelsResponse, seconds: number) {
  if (!levels.peaksDb.length) return null;
  const index = Math.max(
    0,
    Math.min(
      levels.peaksDb.length - 1,
      Math.floor(seconds / levels.intervalSeconds),
    ),
  );
  return {
    peakDb: levels.peaksDb[index],
    rmsDb: levels.rmsDb[index],
    startSeconds: index * levels.intervalSeconds,
  };
}
/** A dB scale keeps low level passages visible; it is also used for the threshold guide. */
export function dbHeight(db: number) {
  return Math.max(0, Math.min(1, (db + 80) / 80));
}
export function suggestThreshold(peakDb: number) {
  return Math.max(-80, Math.min(-5, Math.ceil(peakDb + 3)));
}

// Keep a native horizontal scrollbar without approaching webview layout limits.
const maxTrackWidth = 4_000_000;
const zoomPresets = [
  86400, 43200, 21600, 10800, 3600, 1800, 900, 300, 120, 60,
  30, 15, 5, 2, 1, 0.5, 0.25, 0.1, 0.05, 0.02,
];

export function waveformZoomSpans(duration: number, viewportWidth: number) {
  const narrowest = Math.min(
    duration,
    Math.max(
      0.02,
      Math.min(5, duration / 64),
      (duration * viewportWidth) / maxTrackWidth,
    ),
  );
  if (narrowest >= duration) return [duration];
  return [
    duration,
    ...zoomPresets.filter((span) => span < duration && span > narrowest),
    narrowest,
  ];
}

export function waveformTrackWidth(
  duration: number,
  span: number,
  viewportWidth: number,
) {
  if (span <= 0 || span >= duration) return viewportWidth;
  return Math.max(
    viewportWidth,
    Math.min(maxTrackWidth, Math.round((duration / span) * viewportWidth)),
  );
}

export function waveformVisibleRange(
  duration: number,
  trackWidth: number,
  viewportWidth: number,
  scrollLeft: number,
) {
  if (!trackWidth) return { startSeconds: 0, endSeconds: duration };
  const start = Math.max(0, Math.min(trackWidth - viewportWidth, scrollLeft));
  return {
    startSeconds: (start / trackWidth) * duration,
    endSeconds: (Math.min(trackWidth, start + viewportWidth) / trackWidth) * duration,
  };
}

export function waveformScrollOffset(
  seconds: number,
  screenX: number,
  duration: number,
  trackWidth: number,
  viewportWidth: number,
) {
  if (!duration) return 0;
  return Math.max(
    0,
    Math.min(trackWidth - viewportWidth, (seconds / duration) * trackWidth - screenX),
  );
}

const rulerSteps = [
  0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5,
  1, 2, 5, 10, 15, 20, 30, 60, 120, 300, 600, 900,
  1800, 3600, 7200, 10800, 21600, 43200, 86400,
];

export function waveformRulerTicks(from: number, to: number, viewportWidth: number) {
  const ideal = (to - from) / Math.max(2, Math.floor(viewportWidth / 90));
  const step =
    rulerSteps.find((value) => value >= ideal) ?? Math.ceil(ideal / 86400) * 86400;
  const first = Math.ceil(from / step - 1e-9);
  const last = Math.floor(to / step + 1e-9);
  const seconds = Array.from(
    { length: Math.max(0, last - first + 1) },
    (_, offset) => (first + offset) * step,
  );
  return { step, seconds };
}

const envelopeBlockSize = 64;

export function createWaveformEnvelope({
  intervalSeconds,
  peaksDb,
  rmsDb,
}: WaveformLevelsResponse) {
  const powers = Float64Array.from(rmsDb, (db) => 10 ** (db / 10));
  const blockPeaks = new Float64Array(
    Math.ceil(peaksDb.length / envelopeBlockSize),
  ).fill(-120);
  const blockPowers = new Float64Array(blockPeaks.length);
  for (let index = 0; index < peaksDb.length; index++) {
    const block = Math.floor(index / envelopeBlockSize);
    blockPeaks[block] = Math.max(blockPeaks[block], peaksDb[index]);
    blockPowers[block] += powers[index];
  }

  return (startSeconds: number, endSeconds: number) => {
    const from = Math.max(0, startSeconds / intervalSeconds);
    const to = Math.min(peaksDb.length, endSeconds / intervalSeconds);
    if (to <= from) return { peakDb: -120, rmsDb: -120 };
    const boundaryTolerance = 1e-9;
    const first = Math.floor(from + boundaryTolerance);
    const end = Math.ceil(to - boundaryTolerance);
    let peakDb = -120;
    let power = 0;
    for (let index = first; index < end;) {
      if (
        index % envelopeBlockSize === 0 &&
        index >= from - boundaryTolerance &&
        index + envelopeBlockSize <= to + boundaryTolerance
      ) {
        const block = index / envelopeBlockSize;
        peakDb = Math.max(peakDb, blockPeaks[block]);
        power += blockPowers[block];
        index += envelopeBlockSize;
      } else {
        peakDb = Math.max(peakDb, peaksDb[index]);
        const overlap = Math.min(index + 1, to) - Math.max(index, from);
        power += powers[index] * overlap;
        index++;
      }
    }
    return { peakDb, rmsDb: Math.max(-120, 10 * Math.log10(power / (to - from))) };
  };
}
