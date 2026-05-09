use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};

use crate::debug_log;
use crate::modules::downloader::domain::entities::{
    AudioQuality, DownloadMode, DownloadPreset, DownloadProgress, DownloadRequest, VideoQuality,
};
use crate::modules::downloader::domain::errors::DownloaderError;
use crate::modules::downloader::domain::ports::DownloadPort;

use super::dependencies::yt_dlp_command;

pub struct YtDlpAdapter;

fn get_title_impl(
    url: &str,
    cookies_from_browser: Option<&str>,
    js_runtime: &str,
) -> Result<String, DownloaderError> {
    let cmd = yt_dlp_command();
    debug_log!("[yt-dlp] get_title start cmd={} url={}", cmd, url);
    let mut command = command_with_hidden_window(std::process::Command::new(&cmd));
    command.args(["--ignore-config", "--flat-playlist", "--print", "%(title)s"]);
    append_cookies_from_browser(&mut command, cookies_from_browser);
    append_js_runtime(&mut command, js_runtime);
    command.arg(url);
    let output = command
        .output()
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;

    if !output.status.success() {
        debug_log!("[yt-dlp] get_title failed status={}", output.status);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !stderr.trim().is_empty() {
            debug_log!("[yt-dlp] get_title stderr={}", stderr.trim());
        }
        return Err(DownloaderError::ProcessFailed(
            stderr,
        ));
    }

    let title = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if title.is_empty() {
        debug_log!("[yt-dlp] get_title empty title");
        return Err(DownloaderError::ProcessFailed("empty title".to_string()));
    }

    debug_log!("[yt-dlp] get_title ok title_len={}", title.len());

    Ok(title)
}

impl DownloadPort for YtDlpAdapter {
    fn run_download(
        &self,
        request: &DownloadRequest,
        ffmpeg_path: &str,
        js_runtime: &str,
        on_progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), DownloaderError> {
        on_progress(DownloadProgress {
            fraction: 0.05,
            message: "Starting download".to_string(),
        });

        let mut cmd = command_with_hidden_window(Command::new(yt_dlp_command()));
        debug_log!(
            "[yt-dlp] run_download start url={} out={} mode={:?} preset={:?} vq={:?} aq={:?} ffmpeg={}",
            request.url,
            request.output_path,
            request.mode,
            request.preset,
            request.video_quality,
            request.audio_quality,
            ffmpeg_path
        );
        cmd.arg("--newline")
            .arg("--ignore-config")
            .arg("--progress")
            .arg("--ffmpeg-location")
            .arg(ffmpeg_path)
            .arg("-o")
            .arg(&request.output_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        append_cookies_from_browser(&mut cmd, request.cookies_from_browser.as_deref());
        append_js_runtime(&mut cmd, js_runtime);
        cmd.arg(&request.url);

        match request.mode {
            DownloadMode::AudioOnlyMp3 => {
                cmd.args([
                    "-x",
                    "--audio-format",
                    "mp3",
                    "--audio-quality",
                    audio_quality_value(request.audio_quality),
                ]);
            }
            DownloadMode::VideoWithAudio => {
                cmd.arg("-f").arg(video_audio_format(
                    request.video_quality,
                    request.audio_quality,
                    request.preset,
                ));
                cmd.args(["--merge-output-format", merge_format(request.preset)]);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        debug_log!("[yt-dlp] run_download spawned pid={}", child.id());

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DownloaderError::ProcessFailed("missing stdout".to_string()))?;
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            let line = line.map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            if let Some(p) = parse_progress(&line) {
                on_progress(DownloadProgress {
                    fraction: p,
                    message: format!("Downloading {:.0}%", p * 100.0),
                });
            }
        }

        let status = child
            .wait()
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        if status.success() {
            debug_log!("[yt-dlp] run_download success status={}", status);
            on_progress(DownloadProgress {
                fraction: 1.0,
                message: "Finished".to_string(),
            });
            Ok(())
        } else {
            let mut stderr_text = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut stderr_text);
            }
            let stderr_trimmed = stderr_text.trim().to_string();
            if !stderr_text.trim().is_empty() {
                debug_log!("[yt-dlp] run_download stderr={}", stderr_text.trim());
            }
            debug_log!("[yt-dlp] run_download failed status={}", status);
            let message = if stderr_trimmed.is_empty() {
                format!("yt-dlp exited with {status}")
            } else {
                format!("yt-dlp exited with {status}: {stderr_trimmed}")
            };
            Err(DownloaderError::ProcessFailed(message))
        }
    }

    fn get_title(
        &self,
        url: &str,
        cookies_from_browser: Option<&str>,
        js_runtime: &str,
    ) -> Result<String, DownloaderError> {
        get_title_impl(url, cookies_from_browser, js_runtime)
    }
}

fn append_cookies_from_browser(cmd: &mut Command, cookies_from_browser: Option<&str>) {
    if let Some(browser) = cookies_from_browser {
        if !browser.trim().is_empty() {
            cmd.arg("--cookies-from-browser").arg(browser);
        }
    }
}

fn append_js_runtime(cmd: &mut Command, js_runtime: &str) {
    if !js_runtime.trim().is_empty() {
        cmd.arg("--js-runtimes").arg(js_runtime);
    }
}

fn command_with_hidden_window(command: Command) -> Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = command;
        command.creation_flags(CREATE_NO_WINDOW);
        return command;
    }

    command
}

fn video_audio_format(video: VideoQuality, audio: AudioQuality, preset: DownloadPreset) -> String {
    let v = match video {
        VideoQuality::Best => "bestvideo",
        VideoQuality::P1080 => "bestvideo[height<=1080]",
        VideoQuality::P720 => "bestvideo[height<=720]",
        VideoQuality::P480 => "bestvideo[height<=480]",
    };
    let a = match audio {
        AudioQuality::Best => "bestaudio",
        AudioQuality::K320 => "bestaudio[abr<=320]",
        AudioQuality::K192 => "bestaudio[abr<=192]",
        AudioQuality::K128 => "bestaudio[abr<=128]",
    };

    match preset {
        DownloadPreset::Compatibility => {
            let v_compat = match video {
                VideoQuality::Best => "bestvideo[ext=mp4][vcodec^=avc1]",
                VideoQuality::P1080 => "bestvideo[ext=mp4][vcodec^=avc1][height<=1080]",
                VideoQuality::P720 => "bestvideo[ext=mp4][vcodec^=avc1][height<=720]",
                VideoQuality::P480 => "bestvideo[ext=mp4][vcodec^=avc1][height<=480]",
            };
            let a_compat = match audio {
                AudioQuality::Best => "bestaudio[ext=m4a]",
                AudioQuality::K320 => "bestaudio[ext=m4a][abr<=320]",
                AudioQuality::K192 => "bestaudio[ext=m4a][abr<=192]",
                AudioQuality::K128 => "bestaudio[ext=m4a][abr<=128]",
            };
            format!("{v_compat}+{a_compat}/{v}+{a}/best[ext=mp4]/best")
        }
        DownloadPreset::MaxQuality => {
            format!("{v}+{a}/best")
        }
    }
}

fn merge_format(preset: DownloadPreset) -> &'static str {
    match preset {
        DownloadPreset::Compatibility => "mp4",
        DownloadPreset::MaxQuality => "mkv",
    }
}

fn audio_quality_value(audio: AudioQuality) -> &'static str {
    match audio {
        AudioQuality::Best => "0",
        AudioQuality::K320 => "320K",
        AudioQuality::K192 => "192K",
        AudioQuality::K128 => "128K",
    }
}

pub fn parse_progress(line: &str) -> Option<f32> {
    let marker = "[download]";
    if !line.contains(marker) || !line.contains('%') {
        return None;
    }
    let percent_idx = line.find('%')?;
    let prefix = &line[..percent_idx];
    let num = prefix.split_whitespace().last()?;
    let val: f32 = num.parse().ok()?;
    Some((val / 100.0).clamp(0.0, 1.0))
}
