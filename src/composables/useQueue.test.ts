import { describe, expect, it } from "vitest";
import type { JobProgressPayload, QueueItemData } from "../types/jobs";
import { applyJobProgress, getQueueStats, isActiveStatus, isTerminalStatus } from "./useQueue";

function queueItem(overrides: Partial<QueueItemData> = {}): QueueItemData {
  return {
    id: "job-1",
    kind: "download",
    mediaKind: "audio",
    format: "mp3",
    quality: "best",
    status: "waiting",
    percent: 0,
    speed: "-",
    eta: "-",
    totalSize: "-",
    title: "Source",
    source: "https://example.test/source",
    request: {
      kind: "download",
      url: "https://example.test/source",
      format: "mp3",
      quality: "best",
      outputPath: "/tmp",
      playlistMode: "single",
      audioTarget: "general",
      videoTarget: "general",
    },
    ...overrides,
  };
}

describe("queue helpers", () => {
  it("classifies active and terminal statuses", () => {
    expect(isActiveStatus("downloading")).toBe(true);
    expect(isActiveStatus("waiting")).toBe(false);
    expect(isTerminalStatus("done")).toBe(true);
    expect(isTerminalStatus("processing")).toBe(false);
  });

  it("summarizes queue counts", () => {
    expect(
      getQueueStats([
        queueItem(),
        queueItem({ id: "job-2", status: "downloading" }),
        queueItem({ id: "job-3", status: "done" }),
      ]),
    ).toEqual({ total: 3, active: 1, waiting: 1 });
  });

  it("applies backend progress payloads without overwriting absent optional fields", () => {
    const item = queueItem({ title: "Original", detail: "Queued" });
    const payload: JobProgressPayload = {
      id: "job-1",
      jobKind: "download",
      mediaKind: "audio",
      status: "downloading",
      percent: 42,
      speed: "1MiB/s",
      eta: "00:05",
      totalSize: "10MiB",
      title: null,
      detail: "Downloading",
      outputPath: null,
      error: null,
    };

    applyJobProgress(item, payload);

    expect(item.status).toBe("downloading");
    expect(item.percent).toBe(42);
    expect(item.title).toBe("Original");
    expect(item.detail).toBe("Downloading");
    expect(item.outputPath).toBeUndefined();
  });
});
