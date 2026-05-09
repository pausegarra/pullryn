use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use serde_json::Value;
#[cfg(target_os = "linux")]
use tar::Archive;
#[cfg(target_os = "linux")]
use xz2::read::XzDecoder;
use zip::ZipArchive;

use crate::debug_log;
use crate::modules::downloader::domain::errors::DownloaderError;
use crate::modules::downloader::domain::ports::DependencyPort;

const DEPENDENCY_CONNECT_TIMEOUT_SECS: u64 = 8;

pub struct SystemDependencies;

impl DependencyPort for SystemDependencies {
    fn ensure_yt_dlp(&self) -> Result<String, DownloaderError> {
        debug_log!(
            "[deps] ensure_yt_dlp home={} local_path={} exists={}",
            home_dir().display(),
            local_yt_dlp_path().display(),
            local_yt_dlp_path().exists()
        );
        if command_works("yt-dlp") {
            debug_log!("[deps] ensure_yt_dlp using PATH yt-dlp");
            return Ok("yt-dlp".to_string());
        }
        if local_yt_dlp_path().exists() {
            if local_yt_dlp_works() {
                debug_log!("[deps] ensure_yt_dlp using local {}", local_yt_dlp_path().display());
                return Ok(local_yt_dlp_path().display().to_string());
            }

            debug_log!(
                "[deps] ensure_yt_dlp local exists but not runnable {}; reinstalling",
                local_yt_dlp_path().display()
            );
        }
        debug_log!("[deps] ensure_yt_dlp local missing; installing");
        install_local_yt_dlp()?;
        if command_works("yt-dlp") {
            debug_log!("[deps] ensure_yt_dlp install done; PATH yt-dlp available");
            Ok("yt-dlp".to_string())
        } else if local_yt_dlp_path().exists() && local_yt_dlp_works() {
            debug_log!(
                "[deps] ensure_yt_dlp install done; local available {}",
                local_yt_dlp_path().display()
            );
            Ok(local_yt_dlp_path().display().to_string())
        } else {
            debug_log!("[deps] ensure_yt_dlp failed after install");
            Err(DownloaderError::YtDlpUnavailable)
        }
    }

    fn ensure_ffmpeg(&self) -> Result<String, DownloaderError> {
        debug_log!(
            "[deps] ensure_ffmpeg local_path={} exists={}",
            local_ffmpeg_path().display(),
            local_ffmpeg_path().exists()
        );
        if ffmpeg_command_works("ffmpeg") {
            debug_log!("[deps] ensure_ffmpeg using PATH ffmpeg");
            return Ok("ffmpeg".to_string());
        }

        ensure_local_ffmpeg_suite()?;
        if local_ffmpeg_path().exists() {
            Ok(local_ffmpeg_path().display().to_string())
        } else {
            Err(DownloaderError::FfmpegUnavailable)
        }
    }

    fn ensure_ffprobe(&self) -> Result<String, DownloaderError> {
        debug_log!(
            "[deps] ensure_ffprobe local_path={} exists={}",
            local_ffprobe_path().display(),
            local_ffprobe_path().exists()
        );
        if ffmpeg_command_works("ffprobe") {
            debug_log!("[deps] ensure_ffprobe using PATH ffprobe");
            return Ok("ffprobe".to_string());
        }

        ensure_local_ffmpeg_suite()?;
        if local_ffprobe_path().exists() {
            Ok(local_ffprobe_path().display().to_string())
        } else {
            Err(DownloaderError::FfprobeUnavailable)
        }
    }
}

pub fn yt_dlp_command() -> String {
    if command_works("yt-dlp") {
        let cmd = "yt-dlp".to_string();
        debug_log!("[deps] yt_dlp_command selected={}", cmd);
        cmd
    } else {
        let cmd = local_yt_dlp_path().display().to_string();
        debug_log!("[deps] yt_dlp_command selected={}", cmd);
        cmd
    }
}

fn command_works(name: &str) -> bool {
    let ok = command_works_with_arg(name, "--version");
    debug_log!("[deps] command_works name={} ok={}", name, ok);
    ok
}

fn ffmpeg_command_works(name: &str) -> bool {
    let ok = command_works_with_arg(name, "-version");
    debug_log!("[deps] ffmpeg_command_works name={} ok={}", name, ok);
    ok
}

fn command_works_with_arg(name: &str, arg: &str) -> bool {
    command_with_hidden_window(Command::new(name))
        .arg(arg)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_local_ffmpeg_suite() -> Result<(), DownloaderError> {
    if local_ffmpeg_path().exists() && local_ffprobe_path().exists() {
        return Ok(());
    }

    let bin_dir = local_bin_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;

    #[cfg(target_os = "windows")]
    {
        install_windows_ffmpeg()?;
    }

    #[cfg(target_os = "linux")]
    {
        install_linux_ffmpeg()?;
    }

    #[cfg(target_os = "macos")]
    {
        install_macos_ffmpeg()?;
    }

    if !local_ffmpeg_path().exists() || !local_ffprobe_path().exists() {
        return Err(DownloaderError::FfmpegUnavailable);
    }

    validate_binary_with_retry(&local_ffmpeg_path(), "ffmpeg")?;
    validate_binary_with_retry(&local_ffprobe_path(), "ffprobe")?;

    Ok(())
}

fn validate_binary_with_retry(path: &Path, name: &str) -> Result<(), DownloaderError> {
    let mut last_error = String::new();

    for attempt in 1..=5 {
        match command_with_hidden_window(Command::new(path))
            .arg("-version")
            .output()
        {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                last_error = format!(
                    "{} -version exited with {} (attempt {attempt}/5): {}",
                    name,
                    output.status,
                    stderr.trim()
                );
            }
            Err(err) => {
                last_error = format!(
                    "failed to run {} at {} (attempt {attempt}/5): {}",
                    name,
                    path.display(),
                    err
                );
            }
        }

        thread::sleep(Duration::from_millis(250));
    }

    Err(DownloaderError::ProcessFailed(last_error))
}

fn install_local_yt_dlp() -> Result<(), DownloaderError> {
    let bin_dir = local_bin_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
    let target = local_yt_dlp_path();
    debug_log!("[deps] install_local_yt_dlp target={}", target.display());

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "iwr https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe -OutFile '{}'",
            target.display()
        );
        let status = command_with_hidden_window(Command::new("powershell"))
            .arg("-Command")
            .arg(script)
            .status()
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        if !status.success() {
            debug_log!("[deps] install_local_yt_dlp powershell failed status={}", status);
            return Err(DownloaderError::YtDlpUnavailable);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let download_url = yt_dlp_download_url();
        debug_log!("[deps] install_local_yt_dlp url={}", download_url);

        let status = command_with_hidden_window(Command::new("curl"))
            .args([
                "-L",
                download_url,
                "-o",
            ])
            .arg(target.display().to_string())
            .status()
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        if !status.success() {
            debug_log!("[deps] install_local_yt_dlp curl failed status={}", status);
            return Err(DownloaderError::YtDlpUnavailable);
        }
        let _ = command_with_hidden_window(Command::new("chmod"))
            .args(["+x", &target.display().to_string()])
            .status();

        #[cfg(target_os = "macos")]
        {
            clear_quarantine(&target);
        }
    }

    Ok(())
}

fn local_bin_dir() -> PathBuf {
    home_dir().join(".pullyt")
}

fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(user_profile) = env::var("USERPROFILE") {
            return PathBuf::from(user_profile);
        }

        if let (Ok(home_drive), Ok(home_path)) = (env::var("HOMEDRIVE"), env::var("HOMEPATH")) {
            return PathBuf::from(format!("{home_drive}{home_path}"));
        }
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home);
    }

    PathBuf::from(".")
}

fn local_yt_dlp_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return local_bin_dir().join("yt-dlp.exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        local_bin_dir().join("yt-dlp")
    }
}

#[cfg(target_os = "macos")]
fn yt_dlp_download_url() -> &'static str {
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
}

#[cfg(not(target_os = "macos"))]
fn yt_dlp_download_url() -> &'static str {
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
}

fn local_yt_dlp_works() -> bool {
    command_works_with_arg(&local_yt_dlp_path().display().to_string(), "--version")
}

fn local_ffmpeg_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return local_bin_dir().join("ffmpeg.exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        local_bin_dir().join("ffmpeg")
    }
}

fn local_ffprobe_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return local_bin_dir().join("ffprobe.exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        local_bin_dir().join("ffprobe")
    }
}

fn local_ffmpeg_license_path() -> PathBuf {
    local_bin_dir().join("ffmpeg.LICENSE")
}

fn local_ffmpeg_readme_path() -> PathBuf {
    local_bin_dir().join("ffmpeg.README")
}

fn download_bytes(url: &str) -> Result<Vec<u8>, DownloaderError> {
    debug_log!("[deps] download start: {url}");
    let started = Instant::now();

    let tmp_path = local_bin_dir().join(".download_tmp");
    if tmp_path.exists() {
        let _ = fs::remove_file(&tmp_path);
    }

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "iwr '{}' -OutFile '{}'",
            url,
            tmp_path.display()
        );
        let status = command_with_hidden_window(Command::new("powershell"))
            .arg("-Command")
            .arg(&script)
            .status()
            .map_err(|e| DownloaderError::ProcessFailed(format!("download failed ({url}): {e}")))?;
        if !status.success() {
            let _ = fs::remove_file(&tmp_path);
            return Err(DownloaderError::ProcessFailed(format!(
                "download failed ({url}): powershell exited with {status}"
            )));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let status = command_with_hidden_window(Command::new("curl"))
            .args([
                "-L",
                "-f",
                "--connect-timeout",
                &DEPENDENCY_CONNECT_TIMEOUT_SECS.to_string(),
                "--max-time",
                "300",
                "-o",
            ])
            .arg(tmp_path.display().to_string())
            .arg(url)
            .status()
            .map_err(|e| DownloaderError::ProcessFailed(format!("download failed ({url}): {e}")))?;
        if !status.success() {
            let _ = fs::remove_file(&tmp_path);
            return Err(DownloaderError::ProcessFailed(format!(
                "download failed ({url}): curl exited with {status}"
            )));
        }
    }

    let data = fs::read(&tmp_path).map_err(|e| {
        DownloaderError::ProcessFailed(format!("failed to read downloaded file: {e}"))
    })?;
    let _ = fs::remove_file(&tmp_path);

    let elapsed = started.elapsed();
    debug_log!(
        "[deps] download complete: {url} ({} bytes in {:.1}s)",
        data.len(),
        elapsed.as_secs_f64()
    );
    Ok(data)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), DownloaderError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| DownloaderError::ProcessFailed(e.to_string()))
}

#[cfg(target_os = "macos")]
fn clear_quarantine(path: &Path) {
    let _ = command_with_hidden_window(Command::new("xattr"))
        .arg("-d")
        .arg("com.apple.quarantine")
        .arg(path)
        .status();
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), DownloaderError> {
    Ok(())
}

fn write_file(path: &Path, data: &[u8]) -> Result<(), DownloaderError> {
    fs::write(path, data).map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;

    #[cfg(target_os = "macos")]
    {
        clear_quarantine(path);
    }

    Ok(())
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

#[cfg(target_os = "windows")]
fn install_windows_ffmpeg() -> Result<(), DownloaderError> {
    let archive = download_bytes(
        "https://github.com/GyanD/codexffmpeg/releases/download/6.1.1/ffmpeg-6.1.1-essentials_build.zip",
    )?;

    let mut zip = ZipArchive::new(Cursor::new(archive))
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;

    extract_zip_file(&mut zip, "ffmpeg.exe", &local_ffmpeg_path())?;
    extract_zip_file(&mut zip, "ffprobe.exe", &local_ffprobe_path())?;
    extract_zip_file(&mut zip, "LICENSE", &local_ffmpeg_license_path())?;
    extract_zip_file(&mut zip, "README.txt", &local_ffmpeg_readme_path())?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn install_linux_ffmpeg() -> Result<(), DownloaderError> {
    let target = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" | "i686" => "i686",
        "arm" => "armhf",
        _ => "amd64",
    };
    let url = format!(
        "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-{target}-static.tar.xz"
    );
    let archive = download_bytes(&url)?;

    let decoder = XzDecoder::new(Cursor::new(archive));
    let mut tar = Archive::new(decoder);

    let mut ffmpeg_written = false;
    let mut ffprobe_written = false;
    let mut license_written = false;
    let mut readme_written = false;

    let entries = tar
        .entries()
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;

    for entry_result in entries {
        let mut entry = entry_result.map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?
            .to_string_lossy()
            .to_string();
        let path_lower = path.to_lowercase();

        if path.ends_with("/ffmpeg") && !ffmpeg_written {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            write_file(&local_ffmpeg_path(), &data)?;
            set_executable(&local_ffmpeg_path())?;
            ffmpeg_written = true;
            continue;
        }

        if path.ends_with("/ffprobe") && !ffprobe_written {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            write_file(&local_ffprobe_path(), &data)?;
            set_executable(&local_ffprobe_path())?;
            ffprobe_written = true;
            continue;
        }

        if path_lower.ends_with("/gplv3.txt") && !license_written {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            write_file(&local_ffmpeg_license_path(), &data)?;
            license_written = true;
            continue;
        }

        if path_lower.ends_with("/readme.txt") && !readme_written {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            write_file(&local_ffmpeg_readme_path(), &data)?;
            readme_written = true;
        }
    }

    if !ffmpeg_written || !ffprobe_written {
        return Err(DownloaderError::FfmpegUnavailable);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn install_macos_ffmpeg() -> Result<(), DownloaderError> {
    if std::env::consts::ARCH == "x86_64" {
        install_macos_x64_ffmpeg()
    } else {
        install_macos_arm64_ffmpeg()
    }
}

#[cfg(target_os = "macos")]
fn install_macos_x64_ffmpeg() -> Result<(), DownloaderError> {
    let ffmpeg_info = download_bytes("https://evermeet.cx/ffmpeg/info/ffmpeg/6.1.1")?;
    let ffprobe_info = download_bytes("https://evermeet.cx/ffmpeg/info/ffprobe/6.1.1")?;

    let ffmpeg_url = parse_evermeet_download_url(&ffmpeg_info)?;
    let ffprobe_url = parse_evermeet_download_url(&ffprobe_info)?;

    let ffmpeg_zip = download_bytes(&ffmpeg_url)?;
    let ffprobe_zip = download_bytes(&ffprobe_url)?;

    let mut zip = ZipArchive::new(Cursor::new(ffmpeg_zip))
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
    extract_zip_file_exact(&mut zip, "ffmpeg", &local_ffmpeg_path())?;
    set_executable(&local_ffmpeg_path())?;

    let mut zip = ZipArchive::new(Cursor::new(ffprobe_zip))
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
    extract_zip_file_exact(&mut zip, "ffprobe", &local_ffprobe_path())?;
    set_executable(&local_ffprobe_path())?;

    let license = download_bytes("https://git.ffmpeg.org/gitweb/ffmpeg.git/blob_plain/HEAD:/LICENSE.md")?;
    write_file(&local_ffmpeg_license_path(), &license)?;

    let readme = download_bytes("https://evermeet.cx/ffmpeg/info/ffmpeg/release")?;
    write_file(&local_ffmpeg_readme_path(), &readme)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn install_macos_arm64_ffmpeg() -> Result<(), DownloaderError> {
    let ffmpeg_zip = download_bytes("https://www.osxexperts.net/ffmpeg6arm.zip")?;
    let ffprobe_zip = download_bytes("https://www.osxexperts.net/ffprobe6arm.zip")?;

    let mut zip = ZipArchive::new(Cursor::new(ffmpeg_zip))
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
    extract_zip_file_exact(&mut zip, "ffmpeg", &local_ffmpeg_path())?;
    set_executable(&local_ffmpeg_path())?;

    let mut zip = ZipArchive::new(Cursor::new(ffprobe_zip))
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
    extract_zip_file_exact(&mut zip, "ffprobe", &local_ffprobe_path())?;
    set_executable(&local_ffprobe_path())?;

    let license = download_bytes("https://git.ffmpeg.org/gitweb/ffmpeg.git/blob_plain/n6.1.1:/LICENSE.md")?;
    write_file(&local_ffmpeg_license_path(), &license)?;

    let readme = download_bytes("https://git.ffmpeg.org/gitweb/ffmpeg.git/blob_plain/n6.1.1:/README.md")?;
    write_file(&local_ffmpeg_readme_path(), &readme)?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn parse_evermeet_download_url(json_bytes: &[u8]) -> Result<String, DownloaderError> {
    let value: Value = serde_json::from_slice(json_bytes)
        .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
    value
        .get("download")
        .and_then(|v| v.get("zip"))
        .and_then(|v| v.get("url"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| DownloaderError::ProcessFailed("invalid evermeet JSON payload".to_string()))
}

#[cfg(target_os = "windows")]
fn extract_zip_file(
    zip: &mut ZipArchive<Cursor<Vec<u8>>>,
    ends_with: &str,
    target: &Path,
) -> Result<(), DownloaderError> {
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        if file.name().ends_with(ends_with) {
            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            write_file(target, &data)?;
            return Ok(());
        }
    }

    Err(DownloaderError::ProcessFailed(format!(
        "missing file in zip: {ends_with}"
    )))
}

#[cfg(target_os = "macos")]
fn extract_zip_file_exact(
    zip: &mut ZipArchive<Cursor<Vec<u8>>>,
    file_name: &str,
    target: &Path,
) -> Result<(), DownloaderError> {
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
        let leaf = Path::new(file.name())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if leaf == file_name {
            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .map_err(|e| DownloaderError::ProcessFailed(e.to_string()))?;
            write_file(target, &data)?;
            return Ok(());
        }
    }

    Err(DownloaderError::ProcessFailed(format!(
        "missing file in zip: {file_name}"
    )))
}
