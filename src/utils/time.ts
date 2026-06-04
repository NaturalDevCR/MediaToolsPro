export const MIN_TRIM_GAP_SECONDS = 0.1;

export type TrimHandle = "start" | "end";

export function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function formatDisplayTime(value: number) {
  const safe = Math.max(0, value);
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;

  const secondLabel =
    Math.abs(seconds - Math.round(seconds)) > 0.05
      ? seconds.toFixed(1).padStart(4, "0")
      : `${Math.round(seconds)}`.padStart(2, "0");

  if (hours > 0) {
    return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${secondLabel}`;
  }

  return `${String(minutes).padStart(2, "0")}:${secondLabel}`;
}

export function parseTimeInput(value: string) {
  const raw = value.trim();
  if (!raw) {
    return null;
  }

  if (!raw.includes(":")) {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : null;
  }

  const parts = raw.split(":").map((part) => Number(part));
  if (parts.some((part) => Number.isNaN(part))) {
    return null;
  }

  let seconds = 0;
  for (const part of parts) {
    seconds = seconds * 60 + part;
  }

  return seconds;
}

export function normalizeTrimBounds(
  duration: number,
  start: number,
  end: number,
  changed: TrimHandle | "none" = "none",
  minGap = MIN_TRIM_GAP_SECONDS,
) {
  const safeDuration = Math.max(0, duration);
  if (safeDuration <= 0) {
    return { start: 0, end: 0 };
  }

  const gap = Math.min(minGap, safeDuration);
  let nextStart = clamp(start, 0, safeDuration);
  let nextEnd = clamp(end, 0, safeDuration);

  if (nextEnd < nextStart) {
    if (changed === "start") {
      nextStart = Math.max(0, nextEnd - gap);
    } else {
      nextEnd = Math.min(safeDuration, nextStart + gap);
    }
  }

  if (nextEnd - nextStart < gap) {
    if (changed === "start") {
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
