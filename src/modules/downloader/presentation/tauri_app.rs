use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter};

use crate::modules::downloader::application::use_cases::{
    BootstrapDependenciesUseCase, DownloadMediaUseCase,
};
use crate::modules::downloader::domain::entities::{
    AudioQuality, DownloadMode, DownloadPreset, DownloadProgress, DownloadRequest, Provider,
    VideoQuality,
};
use crate::modules::downloader::infrastructure::dependencies::SystemDependencies;
use crate::modules::downloader::infrastructure::save_dialog::NativeSaveDialog;
use crate::modules::downloader::infrastructure::yt_dlp::YtDlpAdapter;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequestPayload {
    url: String,
    mode: String,
    preset: String,
    video_quality: String,
    audio_quality: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyReportPayload {
    yt_dlp: String,
    ffmpeg: String,
    ffprobe: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgressPayload {
    fraction: f32,
    message: String,
}

impl From<DownloadProgress> for DownloadProgressPayload {
    fn from(value: DownloadProgress) -> Self {
        Self {
            fraction: value.fraction,
            message: value.message,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadCompletePayload {
    ok: bool,
    error: Option<String>,
}

impl DownloadRequestPayload {
    fn into_domain(self) -> Result<DownloadRequest, String> {
        Ok(DownloadRequest {
            provider: Provider::YouTube,
            mode: parse_mode(&self.mode)?,
            preset: parse_preset(&self.preset)?,
            video_quality: parse_video_quality(&self.video_quality)?,
            audio_quality: parse_audio_quality(&self.audio_quality)?,
            url: self.url,
            output_path: String::new(),
        })
    }
}

fn parse_mode(value: &str) -> Result<DownloadMode, String> {
    match value {
        "video_with_audio" => Ok(DownloadMode::VideoWithAudio),
        "audio_only_mp3" => Ok(DownloadMode::AudioOnlyMp3),
        _ => Err(format!("invalid mode: {value}")),
    }
}

fn parse_preset(value: &str) -> Result<DownloadPreset, String> {
    match value {
        "compatibility" => Ok(DownloadPreset::Compatibility),
        "max_quality" => Ok(DownloadPreset::MaxQuality),
        _ => Err(format!("invalid preset: {value}")),
    }
}

fn parse_video_quality(value: &str) -> Result<VideoQuality, String> {
    match value {
        "best" => Ok(VideoQuality::Best),
        "p1080" => Ok(VideoQuality::P1080),
        "p720" => Ok(VideoQuality::P720),
        "p480" => Ok(VideoQuality::P480),
        _ => Err(format!("invalid video quality: {value}")),
    }
}

fn parse_audio_quality(value: &str) -> Result<AudioQuality, String> {
    match value {
        "best" => Ok(AudioQuality::Best),
        "k320" => Ok(AudioQuality::K320),
        "k192" => Ok(AudioQuality::K192),
        "k128" => Ok(AudioQuality::K128),
        _ => Err(format!("invalid audio quality: {value}")),
    }
}

#[tauri::command]
async fn bootstrap_dependencies() -> Result<DependencyReportPayload, String> {
    eprintln!("[deps] bootstrap_dependencies: start");

    let result = tauri::async_runtime::spawn_blocking(move || {
        let dep = Arc::new(SystemDependencies);
        let use_case = BootstrapDependenciesUseCase::new(dep);
        use_case
            .execute()
            .map(|report| DependencyReportPayload {
                yt_dlp: report.yt_dlp,
                ffmpeg: report.ffmpeg,
                ffprobe: report.ffprobe,
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("dependency bootstrap panicked: {e}"))?;

    eprintln!("[deps] bootstrap_dependencies: completed");
    result
}

#[tauri::command]
fn open_github() {
    let _ = open::that("https://github.com/pausegarra/pullyt");
}

#[tauri::command]
fn start_download(app: AppHandle, payload: DownloadRequestPayload) -> Result<(), String> {
    let request = payload.into_domain()?;
    tauri::async_runtime::spawn(async move {
        let dependencies = Arc::new(SystemDependencies);
        let save = Arc::new(NativeSaveDialog);
        let yt_dlp = Arc::new(YtDlpAdapter);
        let use_case = DownloadMediaUseCase::new(dependencies, save, yt_dlp);

        let result = use_case.execute(request, &mut |progress| {
            let payload: DownloadProgressPayload = progress.into();
            let _ = app.emit("download-progress", payload);
        });

        let done = match result {
            Ok(()) => DownloadCompletePayload {
                ok: true,
                error: None,
            },
            Err(err) => DownloadCompletePayload {
                ok: false,
                error: Some(err.to_string()),
            },
        };

        let _ = app.emit("download-complete", done);
    });

    Ok(())
}

pub fn run() {
    eprintln!("[startup] tauri_app::run starting");
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let close_window = PredefinedMenuItem::close_window(app, None)?;
            let quit = PredefinedMenuItem::quit(app, None)?;
            let undo = PredefinedMenuItem::undo(app, None)?;
            let redo = PredefinedMenuItem::redo(app, None)?;
            let cut = PredefinedMenuItem::cut(app, None)?;
            let copy = PredefinedMenuItem::copy(app, None)?;
            let paste = PredefinedMenuItem::paste(app, None)?;
            let select_all = PredefinedMenuItem::select_all(app, None)?;
            let check_for_updates = MenuItem::with_id(
                app,
                "check_for_updates",
                "Check for updates",
                true,
                None::<&str>,
            )?;
            let file_menu = Submenu::with_items(app, "File", true, &[&close_window, &quit])?;
            let edit_menu =
                Submenu::with_items(app, "Edit", true, &[&undo, &redo, &cut, &copy, &paste, &select_all])?;
            let help_menu = Submenu::with_items(app, "Help", true, &[&check_for_updates])?;
            let menu = Menu::with_items(app, &[&file_menu, &edit_menu, &help_menu])?;
            app.set_menu(menu)?;

            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "check_for_updates" {
                let _ = app.emit("menu-check-for-updates", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_dependencies,
            open_github,
            start_download,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}
