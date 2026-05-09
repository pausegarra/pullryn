#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    YouTube,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadMode {
    VideoWithAudio,
    AudioOnlyMp3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadPreset {
    Compatibility,
    MaxQuality,
}

impl Default for DownloadPreset {
    fn default() -> Self {
        Self::Compatibility
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoQuality {
    Best,
    P1080,
    P720,
    P480,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioQuality {
    Best,
    K320,
    K192,
    K128,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub provider: Provider,
    pub mode: DownloadMode,
    pub preset: DownloadPreset,
    pub video_quality: VideoQuality,
    pub audio_quality: AudioQuality,
    pub url: String,
    pub output_path: String,
    pub cookies_from_browser: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub fraction: f32,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub version: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    UpToDate,
    UpdateAvailable(ReleaseInfo),
}
