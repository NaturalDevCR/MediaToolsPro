use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinaryStatus {
    pub ytdlp: bool,
    pub ytdlp_version: Option<String>,
    pub ffmpeg: bool,
    pub ffmpeg_version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinaryVersions {
    pub ytdlp_version: Option<String>,
    pub ffmpeg_version: Option<String>,
}

fn get_bin_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let path = app
        .path()
        .app_data_dir()
        .expect("failed to get app data dir")
        .join("bin");
    println!("DEBUG: bin_dir resolved to: {:?}", path);
    if !path.exists() {
        println!("DEBUG: Creating bin_dir");
        fs::create_dir_all(&path).expect("failed to create bin dir");
    }
    path
}

fn get_ytdlp_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn get_ffmpeg_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

#[tauri::command]
pub async fn check_binaries<R: Runtime>(app: AppHandle<R>) -> BinaryStatus {
    let bin_dir = get_bin_dir(&app);
    let ytdlp_path = bin_dir.join(get_ytdlp_name());
    let ffmpeg_path = bin_dir.join(get_ffmpeg_name());

    println!("DEBUG: Checking ytdlp exists at: {:?}", ytdlp_path);
    println!("DEBUG: Checking ffmpeg exists at: {:?}", ffmpeg_path);

    BinaryStatus {
        ytdlp: ytdlp_path.exists(),
        ytdlp_version: None,
        ffmpeg: ffmpeg_path.exists(),
        ffmpeg_version: None,
    }
}

#[tauri::command]
pub async fn get_binary_versions<R: Runtime>(app: AppHandle<R>) -> BinaryVersions {
    let bin_dir = get_bin_dir(&app);
    let ytdlp_path = bin_dir.join(get_ytdlp_name());
    let ffmpeg_path = bin_dir.join(get_ffmpeg_name());

    let ytdlp_version = if ytdlp_path.exists() {
        Command::new(&ytdlp_path)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    } else {
        None
    };

    let ffmpeg_version = if ffmpeg_path.exists() {
        Command::new(&ffmpeg_path)
            .arg("-version")
            .output()
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout);
                out.lines().next().map(|s| s.to_string())
            })
    } else {
        None
    };

    BinaryVersions {
        ytdlp_version,
        ffmpeg_version,
    }
}

#[tauri::command]
pub async fn install_ytdlp<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let target = bin_dir.join(get_ytdlp_name());

    let url = if cfg!(target_os = "windows") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
    } else {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    };

    download_file(url, &target).await?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&target)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn install_ffmpeg<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let ffmpeg_target = bin_dir.join(get_ffmpeg_name());

    // Simplified URL logic for demo - in prod use robust release logic
    // Using evermeet for mac, gyan for windows
    let (url, is_zip) = if cfg!(target_os = "macos") {
        ("https://evermeet.cx/ffmpeg/getrelease/zip", true)
    } else if cfg!(target_os = "windows") {
        (
            "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
            true,
        )
    } else {
        // Linux generic static build
        (
            "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz",
            false,
        ) // tar.xz handling requires tar+xz crates
    };

    // For now supporting Zip only for Mac/Win as per standard user request (Mac/Win/Linux). Linux tar.xz is separate complexity.
    // If linux, we might ask user to install via apt/pacman or implement tar.xz later.
    // Let's implement Zip handling (Mac/Win).

    if is_zip {
        let tmp_zip = bin_dir.join("ffmpeg.zip");
        download_file(url, &tmp_zip).await?;

        let file = fs::File::open(&tmp_zip).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name();
            if name.ends_with("ffmpeg") || name.ends_with("ffmpeg.exe") {
                // Found it
                // Note: it might be in a subdir. We flatten it.
                let mut outfile = fs::File::create(&ffmpeg_target).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                break;
            }
        }

        // Cleanup
        let _ = fs::remove_file(tmp_zip);

        #[cfg(unix)]
        {
            if ffmpeg_target.exists() {
                let mut perms = fs::metadata(&ffmpeg_target)
                    .map_err(|e| e.to_string())?
                    .permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&ffmpeg_target, perms).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

async fn download_file(url: &str, path: &PathBuf) -> Result<(), String> {
    let response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    // Interactive progress could go here using response.bytes_stream()
    // For now simple blocking-like download
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_ytdlp<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let target = bin_dir.join(get_ytdlp_name());
    if target.exists() {
        fs::remove_file(target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_ffmpeg<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let target = bin_dir.join(get_ffmpeg_name());
    if target.exists() {
        fs::remove_file(target).map_err(|e| e.to_string())?;
    }
    Ok(())
}
