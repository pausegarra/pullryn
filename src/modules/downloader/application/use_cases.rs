use std::sync::Arc;

use crate::debug_log;
use crate::modules::downloader::domain::entities::{DownloadProgress, DownloadRequest, UpdateStatus};
use crate::modules::downloader::domain::errors::DownloaderError;
use crate::modules::downloader::domain::ports::{DependencyPort, DownloadPort, ReleasePort, SaveDialogPort};
use crate::modules::downloader::domain::value_objects::YoutubeUrl;

#[derive(Debug, Clone)]
pub struct DependencyReport {
    pub yt_dlp: String,
    pub ffmpeg: String,
    pub ffprobe: String,
}

pub struct BootstrapDependenciesUseCase {
    dependency_port: Arc<dyn DependencyPort>,
}

impl BootstrapDependenciesUseCase {
    pub fn new(dependency_port: Arc<dyn DependencyPort>) -> Self {
        Self { dependency_port }
    }

    pub fn execute(&self) -> Result<DependencyReport, DownloaderError> {
        debug_log!("[deps] bootstrap: ensure yt-dlp");
        let yt_dlp = self.dependency_port.ensure_yt_dlp()?;
        debug_log!("[deps] bootstrap: yt-dlp ready at {yt_dlp}");
        debug_log!("[deps] bootstrap: ensure ffmpeg");
        let ffmpeg = self.dependency_port.ensure_ffmpeg()?;
        debug_log!("[deps] bootstrap: ffmpeg ready at {ffmpeg}");
        debug_log!("[deps] bootstrap: ensure ffprobe");
        let ffprobe = self.dependency_port.ensure_ffprobe()?;
        debug_log!("[deps] bootstrap: ffprobe ready at {ffprobe}");
        Ok(DependencyReport {
            yt_dlp,
            ffmpeg,
            ffprobe,
        })
    }
}

pub struct DownloadMediaUseCase {
    save_dialog_port: Arc<dyn SaveDialogPort>,
    download_port: Arc<dyn DownloadPort>,
}

impl DownloadMediaUseCase {
    pub fn new(save_dialog_port: Arc<dyn SaveDialogPort>, download_port: Arc<dyn DownloadPort>) -> Self {
        Self {
            save_dialog_port,
            download_port,
        }
    }

    pub fn execute(
        &self,
        mut request: DownloadRequest,
        ffmpeg_path: &str,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), DownloaderError> {
        let valid = YoutubeUrl::parse(&request.url)?;
        request.url = valid.as_str().to_string();

        on_progress(DownloadProgress {
            fraction: 0.0,
            message: "Preparing download".to_string(),
        });

        let title = self.download_port.get_title(&request.url).unwrap_or_default();

        let out = self
            .save_dialog_port
            .choose_output_file(request.mode, request.preset, &title)
            .ok_or(DownloaderError::SaveCanceled)?;
        request.output_path = out;

        self.download_port
            .run_download(&request, ffmpeg_path, on_progress)
    }
}

pub struct CheckForUpdatesUseCase {
    release_port: Arc<dyn ReleasePort>,
    current_version: String,
}

impl CheckForUpdatesUseCase {
    pub fn new(release_port: Arc<dyn ReleasePort>, current_version: String) -> Self {
        Self {
            release_port,
            current_version,
        }
    }

    pub fn execute(&self) -> UpdateStatus {
        match self.release_port.fetch_latest_release() {
            Ok(release) => {
                let current = parse_version(&self.current_version);
                let latest = parse_version(&release.version);
                if latest > current {
                    UpdateStatus::UpdateAvailable(release)
                } else {
                    UpdateStatus::UpToDate
                }
            }
            Err(_) => UpdateStatus::UpToDate,
        }
    }
}

fn parse_version(v: &str) -> Vec<u64> {
    let stripped = v.strip_prefix('v').unwrap_or(v);
    stripped
        .split('.')
        .filter_map(|segment| segment.parse::<u64>().ok())
        .collect()
}
