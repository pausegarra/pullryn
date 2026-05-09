use super::entities::{DownloadMode, DownloadPreset, DownloadProgress, DownloadRequest, ReleaseInfo};
use super::errors::DownloaderError;

pub trait DependencyPort: Send + Sync {
    fn ensure_yt_dlp(&self) -> Result<String, DownloaderError>;
    fn ensure_ffmpeg(&self) -> Result<String, DownloaderError>;
    fn ensure_ffprobe(&self) -> Result<String, DownloaderError>;
    fn ensure_js_runtime(&self) -> Result<String, DownloaderError>;
}

pub trait SaveDialogPort: Send + Sync {
    fn choose_output_file(
        &self,
        mode: DownloadMode,
        preset: DownloadPreset,
        url: &str,
    ) -> Option<String>;
}

pub trait DownloadPort: Send + Sync {
    fn run_download(
        &self,
        request: &DownloadRequest,
        ffmpeg_path: &str,
        js_runtime: &str,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), DownloaderError>;

    fn get_title(
        &self,
        url: &str,
        cookies_from_browser: Option<&str>,
        js_runtime: &str,
    ) -> Result<String, DownloaderError>;
}

pub trait ReleasePort: Send + Sync {
    fn fetch_latest_release(&self) -> Result<ReleaseInfo, DownloaderError>;
}
