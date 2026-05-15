use crate::jobs::{
    emit_job_progress, emit_log, media_kind_for_format, JobProgressPayload, JobState,
};
use regex::Regex;
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

    let mut args = vec![
        "--dump-single-json".to_string(),
        "--no-download".to_string(),
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
    ];

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
    let has_cookies = cookies_path
        .as_ref()
        .map(|p| Path::new(p).is_file())
        .unwrap_or(false);

    if is_youtube_url(&request.url) && !has_cookies {
        // Fallback extractor for public videos when no cookies are available
        // to avoid PO-Token gated 403 failures.
        args.push("--extractor-args".to_string());
        args.push("youtube:player_client=android_vr".to_string());
    }

    if has_cookies {
        if let Some(path) = cookies_path {
            args.push("--cookies".to_string());
            args.push(path);
        }
    }

    args.push(request.url.clone());

    let output = std::process::Command::new(&ytdlp_path)
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

    let title = json["title"].as_str().unwrap_or(&request.url).to_string();
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

    // Also include format entries from info dict if formats array is empty
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
        let result = run_download_process(
            app_handle.clone(),
            request.clone(),
            Arc::clone(&pid),
            Arc::clone(&cancelled),
        )
        .await;

        if let Err(error) = result {
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
                        detail: Some(error.clone()),
                        output_path: None,
                        error: Some(error.clone()),
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
) -> Result<(), String> {
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled before download started".into());
    }

    let azuracast_target = is_azuracast_target(request.audio_target.as_deref());
    let google_tv_cast_target = is_google_tv_cast_target(request.video_target.as_deref());
    let is_audio_request = matches!(
        request.format.as_str(),
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus"
    );
    let effective_format = if azuracast_target && is_audio_request {
        "mp3".to_string()
    } else if google_tv_cast_target && !is_audio_request {
        "mp4".to_string()
    } else {
        request.format.clone()
    };
    let effective_quality = if azuracast_target && is_audio_request {
        "320".to_string()
    } else if google_tv_cast_target && !is_audio_request {
        "2160".to_string()
    } else {
        request.quality.clone()
    };

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

    let mut args = vec![
        "--newline".to_string(),
        "--no-warnings".to_string(),
        "--ffmpeg-location".to_string(),
        ffmpeg_dir.to_string_lossy().to_string(),
        "-P".to_string(),
        request.output_path.clone(),
        "--progress".to_string(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
    ];

    let has_cookies = request
        .cookies_file
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|p| Path::new(p.trim()).is_file())
        .unwrap_or(false);

    if is_youtube_url(&request.url) && !has_cookies {
        // Use a YouTube client that currently avoids PO-Token-gated 403 failures
        // on public media downloads more reliably than the default client selection.
        args.push("--extractor-args".to_string());
        args.push("youtube:player_client=android_vr".to_string());
    }

    if has_cookies {
        args.push("--cookies".to_string());
        args.push(request.cookies_file.as_ref().unwrap().trim().to_string());
    }

    match request.playlist_mode.as_deref() {
        Some("playlist") => args.push("--yes-playlist".to_string()),
        Some("single") => args.push("--no-playlist".to_string()),
        _ if is_youtube_radio_mix_url(&request.url) => args.push("--no-playlist".to_string()),
        _ if request.url.contains("list=") => args.push("--yes-playlist".to_string()),
        _ => args.push("--no-playlist".to_string()),
    }

    if let Some(format_id) = request.format_id.as_ref().filter(|v| !v.is_empty()) {
        // User picked an exact format from the explorer
        args.push("-f".to_string());
        args.push(format_id.clone());

        if is_audio_request {
            args.push("-x".to_string());
            args.push("--audio-format".to_string());
            args.push(audio_format_for_ytdlp(&effective_format).to_string());

            if effective_quality != "best" {
                args.push("--audio-quality".to_string());
                args.push(format!("{}K", effective_quality));
            } else {
                args.push("--audio-quality".to_string());
                args.push("0".to_string());
            }

            if azuracast_target {
                args.push("--postprocessor-args".to_string());
                args.push(
                    "ExtractAudio+ffmpeg_o:-ar 44100 -ac 2 -id3v2_version 3 -write_id3v1 1".to_string(),
                );
            }
        } else {
            args.push("--recode-video".to_string());
            args.push(effective_format.clone());
        }
    } else if matches!(
        effective_format.as_str(),
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus"
    ) {
        args.push("-x".to_string());
        args.push("--audio-format".to_string());
        args.push(audio_format_for_ytdlp(&effective_format).to_string());

        if effective_quality != "best" {
            args.push("--audio-quality".to_string());
            args.push(format!("{}K", effective_quality));
        } else {
            args.push("--audio-quality".to_string());
            args.push("0".to_string());
        }

        if azuracast_target {
            args.push("--postprocessor-args".to_string());
            args.push(
                "ExtractAudio+ffmpeg_o:-ar 44100 -ac 2 -id3v2_version 3 -write_id3v1 1".to_string(),
            );
        }
    } else {
        args.push("--recode-video".to_string());
        args.push(effective_format.clone());

        if google_tv_cast_target {
            args.push("-f".to_string());
            args.push(
                "bestvideo[height<=2160][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height<=2160]+bestaudio/best[height<=2160]/best"
                    .to_string(),
            );
            args.push("--postprocessor-args".to_string());
            args.push(
                "VideoConvertor+ffmpeg_o:-vf scale=3840:2160:force_original_aspect_ratio=decrease,pad=3840:2160:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=24,format=yuv420p -c:v libx264 -preset fast -crf 23 -profile:v high -level 5.1 -c:a aac -b:a 192k -ac 2 -ar 48000 -movflags +faststart"
                    .to_string(),
            );
        } else if effective_quality != "best" {
            args.push("-f".to_string());
            args.push(format!(
                "bestvideo[height<={}]+bestaudio/best[height<={}]/best",
                effective_quality, effective_quality
            ));
        } else {
            args.push("-f".to_string());
            args.push("bestvideo+bestaudio/best".to_string());
        }
    }

    args.push(request.url.clone());

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

    let re_progress = Regex::new(
        r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?([^ ]+)\s+at\s+([^ ]+)\s+ETA\s+([^ ]+)",
    )
    .unwrap();
    let re_percent = Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%").unwrap();

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

        if line.starts_with("[download]") {
            if let Some(caps) = re_progress.captures(&line) {
                emit_job_progress(
                    &app,
                    JobProgressPayload {
                        id: request.id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind_for_format(&request.format).into(),
                        status: "downloading".into(),
                        percent: caps[1].parse::<f64>().unwrap_or(0.0),
                        speed: caps[3].to_string(),
                        eta: caps[4].to_string(),
                        total_size: caps[2].to_string(),
                        title: title.clone(),
                        detail: output_path
                            .clone()
                            .or_else(|| Some("Downloading media".into())),
                        output_path: output_path.clone(),
                        error: None,
                    },
                );
            } else if let Some(percent) = re_percent.captures(&line) {
                emit_job_progress(
                    &app,
                    JobProgressPayload {
                        id: request.id.clone(),
                        job_kind: "download".into(),
                        media_kind: media_kind_for_format(&request.format).into(),
                        status: "downloading".into(),
                        percent: percent[1].parse::<f64>().unwrap_or(0.0),
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
            } else if let Some(destination) = extract_destination(&line) {
                output_path = Some(destination.clone());
                title = Some(
                    Path::new(&destination)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or(&destination)
                        .to_string(),
                );

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
                        detail: Some("Destination resolved".into()),
                        output_path: output_path.clone(),
                        error: None,
                    },
                );
            } else if line.contains("100%") {
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
                        detail: Some("Finalizing file".into()),
                        output_path: output_path.clone(),
                        error: None,
                    },
                );
            }
        } else if !line.starts_with('[') && Path::new(line.trim()).exists() {
            output_path = Some(line.trim().to_string());
            title = Some(file_label(line.trim()));
        } else if line.starts_with("[ExtractAudio]")
            || line.starts_with("[Merger]")
            || line.starts_with("[VideoRemuxer]")
        {
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
                    detail: Some(line.clone()),
                    output_path: output_path.clone(),
                    error: None,
                },
            );
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
        Err(normalize_download_error(&request, stderr_excerpt))
    }
}

fn audio_format_for_ytdlp(format: &str) -> &str {
    match format {
        "ogg" => "vorbis",
        other => other,
    }
}

fn extract_destination(line: &str) -> Option<String> {
    line.split("Destination:")
        .nth(1)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

fn is_azuracast_target(target: Option<&str>) -> bool {
    matches!(target, Some("azuracast"))
}

fn is_google_tv_cast_target(target: Option<&str>) -> bool {
    matches!(target, Some("google_tv_cast"))
}

fn is_youtube_url(url: &str) -> bool {
    url.contains("youtube.com/") || url.contains("youtu.be/")
}

fn is_youtube_radio_mix_url(url: &str) -> bool {
    if !is_youtube_url(url) {
        return false;
    }

    url.contains("start_radio=1") || url.contains("list=RD")
}

fn managed_cookies_file_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("auth")
        .join("cookies.txt"))
}

fn normalize_download_error(request: &DownloadRequest, message: String) -> String {
    if request
        .cookies_file
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .is_some()
        && looks_like_cookie_auth_issue(&message)
    {
        return format!(
            "{} The saved internal cookies.txt may be expired or no longer valid. Export a fresh cookies.txt and import it again from the app.",
            message
        );
    }

    message
}

fn looks_like_cookie_auth_issue(message: &str) -> bool {
    let normalized = message.to_lowercase();

    normalized.contains("sign in")
        || normalized.contains("login")
        || normalized.contains("cookies")
        || normalized.contains("confirm you're not a bot")
        || normalized.contains("age-restricted")
        || normalized.contains("members-only")
        || normalized.contains("authentication")
        || normalized.contains("private video")
        || normalized.contains("premium")
}
