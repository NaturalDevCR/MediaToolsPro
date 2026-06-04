use std::path::Path;
use std::process::Command;
use std::{fs, path::PathBuf};
use tauri::{Manager, Runtime};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    let reveal_target = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };

    #[cfg(target_os = "macos")]
    let status = if target.exists() && !target.is_dir() {
        Command::new("open")
            .args(["-R", &target.to_string_lossy()])
            .status()
    } else {
        Command::new("open").arg(reveal_target).status()
    };

    #[cfg(target_os = "windows")]
    let status = if target.exists() && !target.is_dir() {
        Command::new("explorer")
            .arg(format!("/select,{}", target.to_string_lossy()))
            .status()
    } else {
        Command::new("explorer").arg(reveal_target).status()
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(reveal_target).status();

    status
        .map_err(|error| error.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("The file browser could not be opened.".into())
            }
        })
}

#[tauri::command]
fn prepare_pipeline_temp_dir<R: Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
) -> Result<String, String> {
    let safe_id: String = id
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect();
    let folder_name = if safe_id.is_empty() {
        "pipeline".to_string()
    } else {
        safe_id
    };
    let temp_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("pipelines")
        .join(folder_name);

    fs::create_dir_all(&temp_dir).map_err(|error| error.to_string())?;
    Ok(temp_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn cleanup_pipeline_temp_dir(path: String) -> Result<(), String> {
    let temp_dir = PathBuf::from(path);
    if temp_dir.exists() {
        fs::remove_dir_all(temp_dir).map_err(|error| error.to_string())?;
    }
    Ok(())
}

mod binaries;
mod jobs;
mod media;
mod ytdlp;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(jobs::JobState::new())
        .invoke_handler(tauri::generate_handler![
            greet,
            reveal_path,
            prepare_pipeline_temp_dir,
            cleanup_pipeline_temp_dir,
            binaries::check_binaries,
            binaries::get_binary_versions,
            binaries::install_ytdlp,
            binaries::install_ffmpeg,
            binaries::delete_ytdlp,
            binaries::delete_ffmpeg,
            binaries::check_binary_updates,
            binaries::auto_update_ytdlp,
            jobs::cancel_job,
            ytdlp::list_formats,
            ytdlp::start_download,
            ytdlp::cancel_download,
            ytdlp::import_cookies_file,
            ytdlp::get_saved_cookies_file,
            ytdlp::clear_saved_cookies_file,
            media::probe_media,
            media::render_waveform_preview,
            media::start_media_process
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
