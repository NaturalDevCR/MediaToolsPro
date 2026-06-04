import type { JobProgressPayload, QueueItemData, QueueStatus } from "../types/jobs";

export function isTerminalStatus(status: QueueStatus) {
  return ["done", "error", "cancelled"].includes(status);
}

export function isActiveStatus(status: QueueStatus) {
  return ["downloading", "processing", "converting"].includes(status);
}

export function getQueueStats(items: QueueItemData[]) {
  const active = items.filter((item) => isActiveStatus(item.status)).length;
  const waiting = items.filter((item) => item.status === "waiting").length;
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
