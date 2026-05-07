use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Runtime};

struct RunningJob {
    pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct JobState {
    jobs: Arc<Mutex<HashMap<String, RunningJob>>>,
}

impl JobState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, id: &str) -> Result<(Arc<Mutex<Option<u32>>>, Arc<AtomicBool>), String> {
        let mut jobs = self.jobs.lock().map_err(|_| "Job registry is poisoned")?;

        if jobs.contains_key(id) {
            return Err(format!("Job with id '{}' already exists", id));
        }

        let pid = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));

        jobs.insert(
            id.to_string(),
            RunningJob {
                pid: Arc::clone(&pid),
                cancelled: Arc::clone(&cancelled),
            },
        );

        Ok((pid, cancelled))
    }

    pub fn remove(&self, id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(id);
        }
    }

    pub fn cancel(&self, id: &str) -> Result<bool, String> {
        let job = {
            let mut jobs = self.jobs.lock().map_err(|_| "Job registry is poisoned")?;
            jobs.remove(id)
        };

        let Some(job) = job else {
            return Ok(false);
        };

        job.cancelled.store(true, Ordering::SeqCst);

        if let Ok(pid_guard) = job.pid.lock() {
            if let Some(pid) = *pid_guard {
                let _ = terminate_process(pid);
            }
        }

        Ok(true)
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressPayload {
    pub id: String,
    pub job_kind: String,
    pub media_kind: String,
    pub status: String,
    pub percent: f64,
    pub speed: String,
    pub eta: String,
    pub total_size: String,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct LogPayload {
    pub message: String,
    pub level: String,
}

pub fn emit_log<R: Runtime>(app: &AppHandle<R>, message: impl Into<String>, level: &str) {
    let _ = app.emit(
        "backend-log",
        LogPayload {
            message: message.into(),
            level: level.to_string(),
        },
    );
}

pub fn emit_job_progress<R: Runtime>(app: &AppHandle<R>, payload: JobProgressPayload) {
    let _ = app.emit("job-progress", payload);
}

pub fn media_kind_for_format(format: &str) -> &'static str {
    if is_audio_format(format) {
        "audio"
    } else {
        "video"
    }
}

pub fn is_audio_format(format: &str) -> bool {
    matches!(
        format,
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus"
    )
}

#[tauri::command]
pub async fn cancel_job(state: tauri::State<'_, JobState>, id: String) -> Result<(), String> {
    match state.cancel(&id)? {
        true => Ok(()),
        false => Err(format!("Job '{}' was not found", id)),
    }
}

fn terminate_process(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .map_err(|error| error.to_string())?;

        if status.success() || status.code() == Some(1) {
            return Ok(());
        }

        return Err(format!("Failed to terminate process {}", pid));
    }

    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|error| error.to_string())?;

        if status.success() {
            return Ok(());
        }

        return Err(format!("Failed to terminate process {}", pid));
    }
}
