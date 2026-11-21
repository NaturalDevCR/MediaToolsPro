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
