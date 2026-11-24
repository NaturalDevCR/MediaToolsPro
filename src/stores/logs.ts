import { ref } from 'vue';

export interface LogEntry {
  id: string;
  timestamp: Date;
  level: 'info' | 'warn' | 'error' | 'success';
  scope: 'audio' | 'video' | 'system';
  message: string;
  source?: string;
}

export const logs = ref<LogEntry[]>([]);

export function addLog(
  message: string,
  level: LogEntry['level'] = 'info',
  scope: LogEntry['scope'] = 'system',
  source?: string,
) {
  logs.value.push({
    id: Date.now().toString(36) + Math.random().toString(36).substr(2),
    timestamp: new Date(),
    level,
    scope,
    message,
    source,
  });
}

export function clearLogs() {
  logs.value = [];
}
