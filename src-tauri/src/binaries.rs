use serde::Serialize;
use std::{fs, path::PathBuf, process::Command, time::Duration};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Serialize)]
pub struct BinaryStatus {
    pub ffmpeg: bool,
    pub ffmpeg_version: Option<String>,
}

fn bin_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("bin"))
        .map_err(|e| e.to_string())
}
fn binary_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}
pub fn ffmpeg_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let installed = bin_dir(app)?.join(binary_name());
    let mut candidates = vec![installed, PathBuf::from("ffmpeg")];
    if cfg!(target_os = "macos") {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/ffmpeg"),
            PathBuf::from("/usr/local/bin/ffmpeg"),
        ]);
    }
    candidates
        .into_iter()
        .find(|path| {
            Command::new(path)
                .arg("-version")
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .ok_or_else(|| "FFmpeg no está disponible. Instálalo desde Ajustes.".into())
}
#[tauri::command]
pub async fn check_binaries<R: Runtime>(app: AppHandle<R>) -> BinaryStatus {
    let version = ffmpeg_path(&app)
        .ok()
        .and_then(|path| Command::new(path).arg("-version").output().ok())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(str::to_string)
        });
    BinaryStatus {
        ffmpeg: version.is_some(),
        ffmpeg_version: version,
    }
}
#[tauri::command]
pub async fn install_ffmpeg<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = bin_dir(&app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let url = if cfg!(target_os = "macos") {
        "https://evermeet.cx/ffmpeg/getrelease/zip"
    } else if cfg!(windows) {
        "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
    } else {
        return Err(
            "Instala FFmpeg con el gestor de paquetes de tu sistema y vuelve a comprobar.".into(),
        );
    };
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let staging = dir.join(if cfg!(windows) {
        "ffmpeg.installing.exe"
    } else {
        "ffmpeg.installing"
    });
    let mut found = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        if std::path::Path::new(entry.name())
            .file_name()
            .is_some_and(|name| name == binary_name())
        {
            let mut file = fs::File::create(&staging).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
            found = true;
            break;
        }
    }
    if !found {
        return Err("El paquete no contiene FFmpeg.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let valid = Command::new(&staging)
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !valid {
        let _ = fs::remove_file(staging);
        return Err("No se pudo ejecutar el motor descargado.".into());
    }
    fs::rename(staging, dir.join(binary_name())).map_err(|e| e.to_string())
}
