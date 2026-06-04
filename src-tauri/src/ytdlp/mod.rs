use crate::jobs::{
    emit_job_progress, emit_log, media_kind_for_format, JobProgressPayload, JobState,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

mod args;
mod clients;
mod errors;
mod progress;

use args::{build_download_args, is_youtube_url};
use clients::{should_try_next_client, YOUTUBE_CLIENT_CHAIN};
use errors::normalize_error;
use progress::{parse_progress_line, ProgressEvent};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub id: String,
    pub url: String,
    pub format: String,
    pub quality: String,
    pub format_id: Option<String>,
    pub output_path: String,
    pub playlist_mode: Option<String>,
    pub audio_target: Option<String>,
    pub video_target: Option<String>,
    pub cookies_file: Option<String>,
    pub recode: Option<bool>,
    pub embed_thumbnail: Option<bool>,
    pub embed_metadata: Option<bool>,
    pub embed_chapters: Option<bool>,
    pub embed_subs: Option<bool>,
    pub sub_langs: Option<String>,
    pub sponsorblock: Option<String>,
    pub playlist_items: Option<String>,
    pub output_template: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ListFormatsRequest {
    pub url: String,
    pub cookies_file: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct YtdlpFormatItem {
    pub format_id: String,
    pub ext: String,
    pub resolution: String,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub format_note: String,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct YtdlpFormatsResponse {
    pub title: String,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub formats: Vec<YtdlpFormatItem>,
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn import_cookies_file<R: Runtime>(
    app: AppHandle<R>,
    sourcePath: String,
) -> Result<String, String> {
    let source = Path::new(&sourcePath);
    if !source.is_file() {
        return Err("The selected cookies.txt file does not exist.".into());
    }

    let target = managed_cookies_file_path(&app)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::copy(source, &target).map_err(|error| error.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_saved_cookies_file<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    let target = managed_cookies_file_path(&app)?;
    if target.is_file() {
        Ok(Some(target.to_string_lossy().to_string()))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn clear_saved_cookies_file<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let target = managed_cookies_file_path(&app)?;
    if target.exists() {
        fs::remove_file(target).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_ytdlp_list_formats(
    ytdlp_path: &PathBuf,
    url: &str,
    cookies_file: Option<&str>,
) -> Result<YtdlpFormatsResponse, String> {
    let mut args = vec![
        "--dump-single-json".to_string(),
        "--no-download".to_string(),
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
    ];

    let has_cookies = cookies_file
        .map(|p| Path::new(p.trim()).is_file())
        .unwrap_or(false);

    if is_youtube_url(url) && !has_cookies {
        args.push("--extractor-args".to_string());
        args.push(format!("youtube:player_client={}", YOUTUBE_CLIENT_CHAIN[0]));
    }

    if has_cookies {
        args.push("--cookies".to_string());
        args.push(cookies_file.unwrap().trim().to_string());
    }

    args.push(url.to_string());

    let output = std::process::Command::new(ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let excerpt = stderr
            .lines()
            .filter(|l| !l.trim().is_empty())
            .last()
            .unwrap_or("yt-dlp exited with an error");
        return Err(excerpt.to_string());
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse yt-dlp output: {}", e))?;

    let title = json["title"].as_str().unwrap_or(url).to_string();
    let duration = json["duration"].as_f64();
    let thumbnail = json["thumbnail"].as_str().map(|s| s.to_string());

    let mut formats: Vec<YtdlpFormatItem> = Vec::new();

    if let Some(array) = json["formats"].as_array() {
        for entry in array {
            let format_id = entry["format_id"].as_str().unwrap_or("").to_string();
            if format_id.is_empty() {
                continue;
            }

            let vcodec = entry["vcodec"].as_str().unwrap_or("none");
            let acodec = entry["acodec"].as_str().unwrap_or("none");
            let has_video = vcodec != "none";
            let has_audio = acodec != "none";

            formats.push(YtdlpFormatItem {
                format_id: format_id.clone(),
                ext: entry["ext"].as_str().unwrap_or("").to_string(),
                resolution: entry["resolution"].as_str().unwrap_or("").to_string(),
                fps: entry["fps"].as_f64(),
                vcodec: if has_video { Some(vcodec.to_string()) } else { None },
                acodec: if has_audio { Some(acodec.to_string()) } else { None },
                filesize: entry["filesize"].as_u64(),
                filesize_approx: entry["filesize_approx"].as_u64(),
                format_note: entry["format_note"].as_str().unwrap_or("").to_string(),
                has_video,
                has_audio,
            });
        }
    }

    if formats.is_empty() {
        if let Some(fmt) = json["format_id"].as_str() {
            let vcodec = json["vcodec"].as_str().unwrap_or("none");
            let acodec = json["acodec"].as_str().unwrap_or("none");
            let has_video = vcodec != "none";
            let has_audio = acodec != "none";
            formats.push(YtdlpFormatItem {
                format_id: fmt.to_string(),
                ext: json["ext"].as_str().unwrap_or("").to_string(),
                resolution: json["resolution"].as_str().unwrap_or("").to_string(),
                fps: json["fps"].as_f64(),
                vcodec: if has_video { Some(vcodec.to_string()) } else { None },
                acodec: if has_audio { Some(acodec.to_string()) } else { None },
                filesize: json["filesize"].as_u64(),
                filesize_approx: json["filesize_approx"].as_u64(),
                format_note: json["format_note"].as_str().unwrap_or("").to_string(),
                has_video,
                has_audio,
            });
        }
    }

    Ok(YtdlpFormatsResponse {
        title,
        duration,
        thumbnail,
        formats,
    })
}

#[tauri::command]
pub async fn list_formats<R: Runtime>(
    app: AppHandle<R>,
    request: ListFormatsRequest,
) -> Result<YtdlpFormatsResponse, String> {
    let bin_dir = app.path().app_data_dir().unwrap().join("bin");
    let ytdlp_path = bin_dir.join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });

    if !ytdlp_path.exists() {
        return Err("yt-dlp binary is missing. Install it from Settings first.".into());
    }

    let cookies_path = request
        .cookies_file
        .as_ref()
        .and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

    // Try with cookies first if available
    if let Some(ref path) = cookies_path {
        match run_ytdlp_list_formats(&ytdlp_path, &request.url, Some(path)) {
            Ok(response) => return Ok(response),
            Err(err) => {
                // If it looks like a cookie/public video issue, retry without cookies
                if looks_like_public_video_failure(&err) {
                    emit_log(&app, "Format listing failed with cookies, retrying without cookies...", "warn");
                    return run_ytdlp_list_formats(&ytdlp_path, &request.url, None);
                }
                return Err(err);
            }
        }
    }

    // No cookies, use public fallback
    run_ytdlp_list_formats(&ytdlp_path, &request.url, None)
}

#[tauri::command]
pub async fn start_download<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    request: DownloadRequest,
) -> Result<(), String> {
    let id = request.id.clone();
    let url = request.url.clone();
    let format = request.format.clone();
    let media_kind = media_kind_for_format(&format).to_string();
    let job_state = state.inner().clone();
    let (pid, cancelled) = job_state.register(&id)?;
    let app_handle = app.clone();
    emit_log(&app_handle, format!("Queued download for {}", url), "info");

    tokio::spawn(async move {
        let mut current_request = request.clone();

        let mut last_err: Option<String> = None;
        let max_attempts = if is_youtube_url(&url) && current_request.cookies_file.is_none() {
            YOUTUBE_CLIENT_CHAIN.len()
        } else {
            1
        };

        for attempt in 0..max_attempts {
            match run_download_process(
                app_handle.clone(),
                current_request.clone(),
                Arc::clone(&pid),
                Arc::clone(&cancelled),
                attempt,
            )
            .await
            {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(error) => {
                    if cancelled.load(Ordering::SeqCst) {
                        last_err = Some(error);
                        break;
                    }
                    if attempt + 1 < max_attempts && should_try_next_client(&error) {
                        emit_log(
                            &app_handle,
                            format!("Retrying with next YouTube client ({})", error),
                            "warn",
                        );
                        last_err = Some(error);
                        continue;
                    }
                    last_err = Some(error);
                    break;
                }
            }
        }

        let final_result = if let Some(ref error) = last_err {
            if request.cookies_file.is_some()
                && !cancelled.load(Ordering::SeqCst)
                && looks_like_public_video_failure(error)
            {
                emit_log(
                    &app_handle,
                    format!(
                        "Download failed with cookies ({}), retrying without cookies...",
                        error
                    ),
                    "warn",
                );
                current_request.cookies_file = None;

                emit_job_progress(
                    &app_handle,
                    JobProgressPayload {
                        id: id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind.clone(),
                        status: "downloading".into(),
                        percent: 0.0,
                        speed: "-".into(),
                        eta: "-".into(),
                        total_size: "-".into(),
                        title: Some(url.clone()),
                        detail: Some("Retrying without cookies...".into()),
                        output_path: None,
                        error: None,
                    },
                );

                run_download_process(
                    app_handle.clone(),
                    current_request,
                    Arc::clone(&pid),
                    Arc::clone(&cancelled),
                    0,
                )
                .await
            } else {
                Err(error.clone())
            }
        } else {
            Ok(())
        };

        if let Err(error) = final_result {
            if cancelled.load(Ordering::SeqCst) {
                emit_job_progress(
                    &app_handle,
                    JobProgressPayload {
                        id: id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind.clone(),
                        status: "cancelled".into(),
                        percent: 0.0,
                        speed: "-".into(),
                        eta: "-".into(),
                        total_size: "-".into(),
                        title: Some(url.clone()),
                        detail: Some("Cancelled by user".into()),
                        output_path: None,
                        error: None,
                    },
                );
                emit_log(
                    &app_handle,
                    format!("Download cancelled for {}", url),
                    "warn",
                );
            } else {
                emit_job_progress(
                    &app_handle,
                    JobProgressPayload {
                        id: id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind.clone(),
                        status: "error".into(),
                        percent: 0.0,
                        speed: "-".into(),
                        eta: "-".into(),
                        total_size: "-".into(),
                        title: Some(url.clone()),
                        // Normalize to a friendly, actionable message only here, at the
                        // UI boundary. The raw error already drove the retry predicates.
                        detail: Some(normalize_error(&error, request.cookies_file.is_some())),
                        output_path: None,
                        error: Some(normalize_error(&error, request.cookies_file.is_some())),
                    },
                );
                emit_log(
                    &app_handle,
                    format!("Download failed for {}: {}", url, error),
                    "error",
                );
            }
        }

        job_state.remove(&id);
    });

    Ok(())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, JobState>, id: String) -> Result<(), String> {
    if state.cancel(&id)? {
        Ok(())
    } else {
        Err(format!("Job '{}' was not found", id))
    }
}

async fn run_download_process<R: Runtime>(
    app: AppHandle<R>,
    request: DownloadRequest,
    pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
    attempt_index: usize,
) -> Result<(), String> {
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled before download started".into());
    }

    let bin_dir = app.path().app_data_dir().unwrap().join("bin");
    let ytdlp_path = bin_dir.join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    let ffmpeg_dir = bin_dir.clone();

    if !ytdlp_path.exists() {
        return Err("yt-dlp binary is missing. Install it from Settings first.".into());
    }

    let has_cookies = request
        .cookies_file
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|p| Path::new(p.trim()).is_file())
        .unwrap_or(false);

    let client = if is_youtube_url(&request.url) && !has_cookies {
        Some(YOUTUBE_CLIENT_CHAIN[attempt_index.min(YOUTUBE_CLIENT_CHAIN.len() - 1)])
    } else {
        None
    };
    if let Some(client) = client {
        emit_log(&app, format!("Using YouTube client: {}", client), "info");
    }
    let ffmpeg_dir_arg = ffmpeg_dir.to_string_lossy().to_string();
    let args = build_download_args(&request, &ffmpeg_dir_arg, client, has_cookies);

    let mut child = Command::new(&ytdlp_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    if let Ok(mut guard) = pid.lock() {
        *guard = child.id();
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture yt-dlp stdout".to_string())?;
    let mut reader = BufReader::new(stdout).lines();

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture yt-dlp stderr".to_string())?;
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut buffer = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            buffer.push(line);
        }
        buffer
    });

    let mut title = Some(request.url.clone());
    let mut output_path = None::<String>;

    emit_job_progress(
        &app,
        JobProgressPayload {
            id: request.id.clone(),
            job_kind: "download".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "downloading".into(),
            percent: 0.0,
            speed: "-".into(),
            eta: "-".into(),
            total_size: "-".into(),
            title: title.clone(),
            detail: Some("Starting yt-dlp".into()),
            output_path: None,
            error: None,
        },
    );

    while let Ok(Some(line)) = reader.next_line().await {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Cancelled by user".into());
        }

        match parse_progress_line(&line) {
            Some(ProgressEvent::Progress {
                percent,
                speed,
                eta,
                total,
            }) => {
                emit_job_progress(
                    &app,
                    JobProgressPayload {
                        id: request.id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind_for_format(&request.format).into(),
                        status: "downloading".into(),
                        percent,
                        speed,
                        eta,
                        total_size: total,
                        title: title.clone(),
                        detail: output_path
                            .clone()
                            .or_else(|| Some("Downloading media".into())),
                        output_path: output_path.clone(),
                        error: None,
                    },
                );
            }
            Some(ProgressEvent::Percent(percent)) => {
                emit_job_progress(
                    &app,
                    JobProgressPayload {
                        id: request.id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind_for_format(&request.format).into(),
                        status: "downloading".into(),
                        percent,
                        speed: "-".into(),
                        eta: "-".into(),
                        total_size: "-".into(),
                        title: title.clone(),
                        detail: output_path
                            .clone()
                            .or_else(|| Some("Downloading media".into())),
                        output_path: output_path.clone(),
                        error: None,
                    },
                );
            }
            Some(ProgressEvent::Destination(dest)) => {
                title = Some(file_label(&dest));
                output_path = Some(dest);
            }
            Some(ProgressEvent::PostProcess(detail)) => {
                emit_job_progress(
                    &app,
                    JobProgressPayload {
                        id: request.id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind_for_format(&request.format).into(),
                        status: "converting".into(),
                        percent: 100.0,
                        speed: "-".into(),
                        eta: "00:00".into(),
                        total_size: "-".into(),
                        title: title.clone(),
                        detail: Some(detail),
                        output_path: output_path.clone(),
                        error: None,
                    },
                );
            }
            None => {
                let trimmed = line.trim();
                if !trimmed.starts_with('[') && Path::new(trimmed).exists() {
                    output_path = Some(trimmed.to_string());
                    title = Some(file_label(trimmed));
                }
            }
        }
    }

    let status = child.wait().await.map_err(|error| error.to_string())?;
    let stderr_lines = stderr_task.await.map_err(|error| error.to_string())?;

    if status.success() {
        emit_job_progress(
            &app,
            JobProgressPayload {
                id: request.id.clone(),
                job_kind: "download".into(),
                media_kind: media_kind_for_format(&request.format).into(),
                status: "done".into(),
                percent: 100.0,
                speed: "-".into(),
                eta: "00:00".into(),
                total_size: "-".into(),
                title: title.clone(),
                detail: Some("Download finished".into()),
                output_path: output_path.clone(),
                error: None,
            },
        );
        emit_log(
            &app,
            format!("Download finished for {}", request.url),
            "success",
        );
        Ok(())
    } else {
        let stderr_excerpt = stderr_lines
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "yt-dlp exited with an error".into());
        // Return the RAW yt-dlp error. The retry predicates in `start_download`
        // (should_try_next_client / looks_like_public_video_failure) match on the
        // unmodified keywords; normalization happens only at the UI emit boundary.
        Err(stderr_excerpt)
    }
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

fn managed_cookies_file_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("auth")
        .join("cookies.txt"))
}

fn looks_like_public_video_failure(message: &str) -> bool {
    let normalized = message.to_lowercase();

    // When cookies are present but expired/invalid, the default extractor
    // may fail with generic availability errors on public videos.
    // This signals we should retry without cookies (using android_vr fallback).
    normalized.contains("not available")
        || normalized.contains("video unavailable")
        || normalized.contains("unavailable")
        || normalized.contains("this video is private")
        || normalized.contains("sign in")
        || normalized.contains("confirm you're not a bot")
        || normalized.contains("members-only")
        || normalized.contains("authentication")
        || normalized.contains("premium")
}

#[cfg(test)]
mod request_tests {
    use super::DownloadRequest;

    #[test]
    fn deserializes_legacy_payload_with_defaults() {
        let json = r#"{
            "id":"x","url":"https://youtu.be/a","format":"mp4","quality":"best",
            "outputPath":"/out"
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.recode, None);
        assert_eq!(req.embed_thumbnail, None);
        assert_eq!(req.sponsorblock, None);
    }
}
