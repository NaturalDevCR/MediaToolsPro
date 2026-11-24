import { computed, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  JobProgressPayload,
  ProcessJobRequest,
  QueueItemData,
  Tool,
} from "../types/jobs";
import { addLog } from "./logs";

const KEY = "mediatoolspro.studio.queue.v2";
const finishedStatusLabels = {
  done: "Completado",
  error: "Error",
  cancelled: "Cancelado",
} as const;
export const queue = ref<QueueItemData[]>([]);
export const queueReady = ref(false);
export const runningCount = computed(
  () => queue.value.filter((item) => item.status === "processing").length,
);
export const pendingCount = computed(
  () =>
    queue.value.filter((item) =>
      ["waiting", "processing"].includes(item.status),
    ).length,
);
let pumping = false;
let stopListening: (() => void) | undefined;
let stopPersisting: (() => void) | undefined;
export const basename = (path: string) => path.split(/[/\\]/).pop() || path;

export async function initStudio() {
  if (!isTauri() || queueReady.value) return;
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(KEY) || "[]");
    if (Array.isArray(saved))
      queue.value = saved
        .filter(
          (item) =>
            item &&
            typeof item.id === "string" &&
            item.request &&
            typeof item.request.inputPath === "string",
        )
        .slice(-100)
        .map((item) =>
          ["waiting", "processing"].includes(item.status)
            ? {
                ...item,
                status: "error",
                error:
                  "La sesión anterior se interrumpió. Puedes reintentar la tarea.",
                percent: 0,
              }
            : item,
        );
  } catch {
    addLog("No se pudo recuperar el historial de exportaciones.", "warn");
  }
  stopListening = await listen<JobProgressPayload>(
    "job-progress",
    ({ payload }) => {
      const item = queue.value.find((item) => item.id === payload.id);
      if (!item || item.status !== "processing") return;
      item.status = payload.status;
      item.percent = Math.max(item.percent, Math.min(100, payload.percent));
      item.detail = payload.detail;
      // A partially rendered file must never be exposed as a completed output.
      if (payload.status === "done") item.outputPath = payload.outputPath;
      item.error = payload.error;
      const status = payload.status;
      if (status === "done" || status === "error" || status === "cancelled") {
        addLog(
          payload.detail || finishedStatusLabels[status],
          status === "done"
            ? "success"
            : status === "error"
              ? "error"
              : "warn",
          "system",
          item.title,
        );
        void pump();
      }
    },
  );
  stopPersisting = watch(
    queue,
    () => {
      try {
        localStorage.setItem(KEY, JSON.stringify(queue.value.slice(-100)));
      } catch {
        addLog("No se pudo guardar el historial.", "warn");
      }
    },
    { deep: true },
  );
  queueReady.value = true;
}
export function disposeStudio() {
  stopListening?.();
  stopPersisting?.();
  queueReady.value = false;
}
export function enqueue(requests: ProcessJobRequest[], tool: Tool) {
  if (!queueReady.value)
    throw new Error("El motor de escritorio todavía no está listo.");
  queue.value.push(
    ...requests.map((request) => ({
      id: crypto.randomUUID(),
      tool,
      status: "waiting" as const,
      percent: 0,
      title: basename(request.inputPath),
      request: JSON.parse(JSON.stringify(request)),
    })),
  );
  void pump();
}
async function pump() {
  if (pumping) return;
  pumping = true;
  try {
    while (runningCount.value < 2) {
      const item = queue.value.find((item) => item.status === "waiting");
      if (!item) break;
      item.status = "processing";
      try {
        await invoke("start_media_process", {
          request: { ...item.request, id: item.id },
        });
      } catch (error) {
        if (item.status === "processing") {
          item.status = "error";
          item.error = String(error);
        }
      }
    }
  } finally {
    pumping = false;
  }
  // A short task can finish while start_media_process is still returning.
  if (
    runningCount.value < 2 &&
    queue.value.some((item) => item.status === "waiting")
  )
    void pump();
}
export async function cancelItem(item: QueueItemData) {
  if (item.status === "waiting") {
    item.status = "cancelled";
    return;
  }
  if (item.status === "processing") {
    try {
      await invoke("cancel_job", { id: item.id });
    } catch (error) {
      item.error = String(error);
    }
    return;
  }
  queue.value = queue.value.filter((entry) => entry.id !== item.id);
}
export function retryItem(item: QueueItemData) {
  enqueue([item.request], item.tool);
}
export function clearHistory() {
  queue.value = queue.value.filter((item) =>
    ["waiting", "processing"].includes(item.status),
  );
}
export async function revealOutput(path: string) {
  await invoke("reveal_path", { path });
}
