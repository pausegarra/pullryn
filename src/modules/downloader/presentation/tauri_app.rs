use std::sync::Arc;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::{AppHandle, Emitter};

use crate::debug_log;
use crate::debug_logs;
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
    cookies_from_browser: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyReportPayload {
    yt_dlp: String,
    ffmpeg: String,
    ffprobe: String,
    js_runtime: String,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DebugLogEntryPayload {
    id: u64,
    message: String,
}

static BOOTSTRAPPED_DEPS: OnceLock<RwLock<Option<DependencyReportPayload>>> = OnceLock::new();

fn deps_state() -> &'static RwLock<Option<DependencyReportPayload>> {
    BOOTSTRAPPED_DEPS.get_or_init(|| RwLock::new(None))
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
            cookies_from_browser: parse_cookies_from_browser(&self.cookies_from_browser),
        })
    }
}

fn parse_cookies_from_browser(value: &str) -> Option<String> {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() || normalized == "none" {
        None
    } else {
        Some(normalized)
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
    debug_log!("[deps] bootstrap_dependencies: start");

    let result = tauri::async_runtime::spawn_blocking(move || {
        let dep = Arc::new(SystemDependencies);
        let use_case = BootstrapDependenciesUseCase::new(dep);
        use_case
            .execute()
            .map(|report| DependencyReportPayload {
                yt_dlp: report.yt_dlp,
                ffmpeg: report.ffmpeg,
                ffprobe: report.ffprobe,
                js_runtime: report.js_runtime,
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("dependency bootstrap panicked: {e}"))?;

    let report = result?;
    if let Ok(mut guard) = deps_state().write() {
        *guard = Some(report.clone());
    }
    debug_log!(
        "[deps] bootstrap_dependencies: completed yt_dlp={} ffmpeg={} ffprobe={} js_runtime={}",
        report.yt_dlp,
        report.ffmpeg,
        report.ffprobe,
        report.js_runtime
    );
    Ok(report)
}

#[tauri::command]
fn open_github() {
    let _ = open::that("https://github.com/pausegarra/pullyt");
}

#[tauri::command]
fn get_debug_logs(since_id: Option<u64>) -> Vec<DebugLogEntryPayload> {
    debug_logs::read_since(since_id.unwrap_or(0))
        .into_iter()
        .map(|entry| DebugLogEntryPayload {
            id: entry.id,
            message: entry.message,
        })
        .collect()
}

#[tauri::command]
fn start_download(app: AppHandle, payload: DownloadRequestPayload) -> Result<(), String> {
    let request = payload.into_domain()?;
    let ffmpeg_path = deps_state()
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().map(|deps| deps.ffmpeg.clone()))
        .ok_or_else(|| "dependencies not bootstrapped; restart app".to_string())?;
    let js_runtime = deps_state()
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().map(|deps| deps.js_runtime.clone()))
        .ok_or_else(|| "dependencies not bootstrapped; restart app".to_string())?;

    tauri::async_runtime::spawn(async move {
        let save = Arc::new(NativeSaveDialog);
        let yt_dlp = Arc::new(YtDlpAdapter);
        let use_case = DownloadMediaUseCase::new(save, yt_dlp);

        let result = use_case.execute(request, &ffmpeg_path, &js_runtime, &mut |progress| {
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
    debug_log!("[startup] tauri_app::run starting");
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let close_window = PredefinedMenuItem::close_window(app, None)?;
            let quit = PredefinedMenuItem::quit(app, None)?;
            let relaunch_app = MenuItem::with_id(
                app,
                "relaunch_app",
                "Relaunch app",
                true,
                None::<&str>,
            )?;
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
            let show_debug_logs = MenuItem::with_id(
                app,
                "show_debug_logs",
                "Show Debug logs",
                true,
                None::<&str>,
            )?;
            let file_menu = Submenu::with_items(app, "File", true, &[&close_window, &relaunch_app, &quit])?;
            let edit_menu =
                Submenu::with_items(app, "Edit", true, &[&undo, &redo, &cut, &copy, &paste, &select_all])?;
            let help_menu = Submenu::with_items(app, "Help", true, &[&check_for_updates, &show_debug_logs])?;
            let menu = Menu::with_items(app, &[&file_menu, &edit_menu, &help_menu])?;
            app.set_menu(menu)?;

            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "check_for_updates" {
                let _ = app.emit("menu-check-for-updates", ());
            } else if event.id().as_ref() == "relaunch_app" {
                let _ = app.emit("menu-relaunch-app", ());
            } else if event.id().as_ref() == "show_debug_logs" {
                if app.get_webview_window("debug-logs").is_none() {
                    let _ = WebviewWindowBuilder::new(
                        app,
                        "debug-logs",
                        WebviewUrl::App("index.html".into()),
                    )
                    .title("Pullyt Debug Logs")
                    .inner_size(820.0, 520.0)
                    .resizable(true)
                    .build();
                } else if let Some(win) = app.get_webview_window("debug-logs") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
                debug_log!("[debug] opened debug logs window");
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_dependencies,
            get_debug_logs,
            open_github,
            start_download,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}
