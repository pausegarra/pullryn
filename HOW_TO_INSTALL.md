# How to Install Pullyt

Install Pullyt from the GitHub Release assets for your operating system.

## Release Assets

- Windows: `Pullyt_x64-setup.exe`
- macOS (Apple Silicon): `Pullyt_aarch64.app.tar.gz`
- Linux: `Pullyt_1.1.0_amd64.AppImage`

## Important: App Is Not Signed

Pullyt is currently distributed as an **unsigned app**. Your operating system may warn or block it the first time you run it.

## Windows

1. Download `Pullyt_x64-setup.exe` from the latest Release.
2. Run the installer.
3. Choose installation options if desired.
4. Complete installation.
5. Launch Pullyt.

If Windows SmartScreen appears, click **More info** -> **Run anyway**.

## macOS (Apple Silicon: M1/M2/M3/M4)

1. Download `Pullyt_aarch64.app.tar.gz` from the latest Release.
2. Extract the `.tar.gz` archive.
3. Move `Pullyt.app` into `/Applications`.
4. Open Pullyt from Applications.

If macOS shows a security warning on first launch:

1. Right-click `Pullyt.app`.
2. Click **Open**.
3. Confirm the launch.

If needed, you can also allow it from **System Settings -> Privacy & Security**.

## Linux (AppImage)

1. Download `Pullyt_1.1.0_amd64.AppImage` from the latest Release.
2. Make it executable:

```bash
chmod +x Pullyt_1.1.0_amd64.AppImage
```

3. Run it:

```bash
./Pullyt_1.1.0_amd64.AppImage
```

## First Launch Notes

- On first run, Pullyt may auto-install `yt-dlp`, FFmpeg, and FFprobe to `~/.pullyt/` if they are not already available on your system.
- This can take a short moment depending on your network connection.
