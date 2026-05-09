<script>
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check } from "@tauri-apps/plugin-updater";
  import { onDestroy, onMount } from "svelte";
  import logo from "@assets/logo.svg";

  let currentVersion = "";

  let url = "";
  let mode = "video_with_audio";
  let preset = "compatibility";
  let videoQuality = "best";
  let audioQuality = "best";
  let cookiesFromBrowser = "none";
  let progress = 0;
  let status = "Starting up...";
  let dependencyInfo = "";
  let busy = true;
  let checkingUpdates = false;
  let confirmDialog = {
    open: false,
    message: "",
    title: "",
    confirmLabel: "Confirm",
    cancelLabel: "Cancel",
  };
  let resolveConfirm;
  let infoDialog = {
    open: false,
    title: "",
    message: "",
  };

  let unlistenProgress;
  let unlistenComplete;
  let unlistenMenuCheckUpdates;
  let unlistenMenuRelaunchApp;
  let debugPollingTimer;

  let isDebugWindow = false;
  let debugLogs = [];
  let lastDebugLogId = 0;
  let cookieHintShown = false;

  function withTimeout(promise, timeoutMs, timeoutMessage) {
    let timer;
    return Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(timeoutMessage)), timeoutMs);
      }),
    ]).finally(() => clearTimeout(timer));
  }

  function yieldToPaint() {
    return new Promise((resolve) => setTimeout(resolve, 0));
  }

  function askForConfirmation(message, options = {}) {
    confirmDialog = {
      open: true,
      message,
      title: options.title ?? "Update available",
      confirmLabel: options.confirmLabel ?? "Confirm",
      cancelLabel: options.cancelLabel ?? "Cancel",
    };

    return new Promise((resolve) => {
      resolveConfirm = resolve;
    });
  }

  function onConfirmDialog(choice) {
    if (resolveConfirm) {
      resolveConfirm(choice);
      resolveConfirm = undefined;
    }

    confirmDialog = {
      ...confirmDialog,
      open: false,
    };
  }

  function closeInfoDialog() {
    infoDialog = {
      ...infoDialog,
      open: false,
    };
  }

  function shouldSuggestCookies(errorText) {
    const value = String(errorText || "").toLowerCase();
    return (
      value.includes("sign in to confirm your age") ||
      value.includes("cookies-from-browser") ||
      value.includes("login_required") ||
      value.includes("age-restricted") ||
      value.includes("without authentication")
    );
  }

  const modeOptions = [
    { value: "video_with_audio", label: "Video + Audio" },
    { value: "audio_only_mp3", label: "Audio only (MP3)" },
  ];

  const presetOptions = [
    { value: "compatibility", label: "Compatibility (H.264/AAC)" },
    { value: "max_quality", label: "Max Quality" },
  ];

  const videoOptions = [
    { value: "best", label: "Best" },
    { value: "p1080", label: "1080p" },
    { value: "p720", label: "720p" },
    { value: "p480", label: "480p" },
  ];

  const audioOptions = [
    { value: "best", label: "Best" },
    { value: "k320", label: "320k" },
    { value: "k192", label: "192k" },
    { value: "k128", label: "128k" },
  ];

  const cookiesBrowserOptions = [
    { value: "none", label: "No cookies" },
    { value: "safari", label: "Safari" },
    { value: "chrome", label: "Chrome" },
    { value: "firefox", label: "Firefox" },
    { value: "brave", label: "Brave" },
    { value: "edge", label: "Edge" },
  ];

  async function startApp() {
    busy = true;
    status = "Bootstrapping dependencies...";
    await yieldToPaint();
    console.info("[startup] bootstrap dependencies begin");
    try {
      const startedAt = Date.now();
      const report = await invoke("bootstrap_dependencies");
      dependencyInfo = `Dependencies\nyt-dlp: ${report.ytDlp}\nffmpeg: ${report.ffmpeg}\nffprobe: ${report.ffprobe}\njs runtime: ${report.jsRuntime}`;
      status = "Ready";
      console.info(`[startup] bootstrap dependencies finished in ${Date.now() - startedAt}ms`, report);
    } catch (error) {
      console.error("[startup] bootstrap dependencies failed", error);
      status = `Dependency error: ${String(error)}`;
    } finally {
      busy = false;
    }
  }

  async function onCheckForUpdates() {
    checkingUpdates = true;
    status = "Checking for updates...";
    try {
      const update = await withTimeout(check(), 15000, "Update check timed out");

      if (!update) {
        status = "You are on the latest version.";
        return;
      }

      const confirmed = await askForConfirmation(
        `A new version (${update.version}) is available. Download and install now?`,
        { title: "Install update", confirmLabel: "Install", cancelLabel: "Later" }
      );

      if (!confirmed) {
        status = `Update available: v${update.version}`;
        return;
      }

      status = `Downloading v${update.version}...`;
      await update.downloadAndInstall();

      const restart = await askForConfirmation(
        `Update installed (v${update.version}). Restart Pullyt now?`,
        { title: "Restart required", confirmLabel: "Restart", cancelLabel: "Not now" }
      );

      if (restart) {
        await relaunch();
      } else {
        status = `Update installed (v${update.version}). Restart Pullyt to apply it.`;
      }
    } catch (error) {
      status = `Update check failed: ${String(error)}`;
    } finally {
      checkingUpdates = false;
    }
  }

  async function checkForUpdatesSilently() {
    if (checkingUpdates) {
      return;
    }

    checkingUpdates = true;
    try {
      const update = await withTimeout(check(), 10000, "Silent update check timed out");
      if (!update) {
        return;
      }

      status = `Update available: v${update.version}`;
      const confirmed = await askForConfirmation(
        `A new version (${update.version}) is available. Download and install now?`,
        { title: "Install update", confirmLabel: "Install", cancelLabel: "Later" }
      );

      if (!confirmed) {
        return;
      }

      status = `Downloading v${update.version}...`;
      await update.downloadAndInstall();

      const restart = await askForConfirmation(
        `Update installed (v${update.version}). Restart Pullyt now?`,
        { title: "Restart required", confirmLabel: "Restart", cancelLabel: "Not now" }
      );

      if (restart) {
        await relaunch();
      } else {
        status = `Update installed (v${update.version}). Restart Pullyt to apply it.`;
      }
    } catch (error) {
      console.warn("[updates] silent update check skipped", error);
    } finally {
      checkingUpdates = false;
    }
  }

  async function onDownload() {
    busy = true;
    progress = 0;
    status = "Preparing download...";
    cookieHintShown = false;

    try {
      await invoke("start_download", {
        payload: {
          url,
          mode,
          preset,
          videoQuality,
          audioQuality,
          cookiesFromBrowser,
        },
      });
    } catch (error) {
      busy = false;
      status = `Download failed: ${String(error)}`;
    }
  }

  async function onOpenGithub() {
    await invoke("open_github");
  }

  async function pullDebugLogs() {
    try {
      const entries = await invoke("get_debug_logs", { sinceId: lastDebugLogId });
      if (!entries?.length) {
        return;
      }

      for (const entry of entries) {
        debugLogs = [...debugLogs, `[${entry.id}] ${entry.message}`];
        lastDebugLogId = Math.max(lastDebugLogId, entry.id);
      }

      if (debugLogs.length > 2000) {
        debugLogs = debugLogs.slice(debugLogs.length - 2000);
      }
    } catch (error) {
      debugLogs = [...debugLogs, `log read failed: ${String(error)}`];
    }
  }

  onMount(async () => {
    const currentWindow = getCurrentWebviewWindow();
    isDebugWindow = currentWindow.label === "debug-logs";

    if (isDebugWindow) {
      await pullDebugLogs();
      debugPollingTimer = setInterval(() => {
        void pullDebugLogs();
      }, 500);
      return;
    }

    console.info("[startup] onMount");

    try {
      unlistenProgress = await listen("download-progress", (event) => {
        progress = event.payload.fraction;
        status = event.payload.message;
        if (!cookieHintShown && shouldSuggestCookies(event.payload.message)) {
          cookieHintShown = true;
          infoDialog = {
            open: true,
            title: "Authentication required",
            message:
              "This video may be age-restricted or require sign-in. Set Cookies to your browser (for example Brave/Safari/Chrome) and try again.",
          };
        }
      });
      console.info("[startup] listen download-progress ok");
    } catch (error) {
      console.warn("[startup] listen download-progress failed", error);
    }

    try {
      unlistenComplete = await listen("download-complete", (event) => {
        busy = false;
        if (event.payload.ok) {
          progress = 1;
          status = "Finished";
        } else {
          const errorText = String(event.payload.error || "");
          status = `Download failed: ${errorText}`;
          if (!cookieHintShown && shouldSuggestCookies(errorText)) {
            cookieHintShown = true;
            infoDialog = {
              open: true,
              title: "Authentication required",
              message:
                "This video requires authentication. Set Cookies to your browser (for example Brave/Safari/Chrome) and try again.",
            };
          }
        }
      });
      console.info("[startup] listen download-complete ok");
    } catch (error) {
      console.warn("[startup] listen download-complete failed", error);
    }

    try {
      unlistenMenuCheckUpdates = await listen("menu-check-for-updates", () => {
        void onCheckForUpdates();
      });
      console.info("[startup] listen menu-check-for-updates ok");
    } catch (error) {
      console.warn("[startup] listen menu-check-for-updates failed", error);
    }

    try {
      unlistenMenuRelaunchApp = await listen("menu-relaunch-app", () => {
        void relaunch();
      });
      console.info("[startup] listen menu-relaunch-app ok");
    } catch (error) {
      console.warn("[startup] listen menu-relaunch-app failed", error);
    }

    try {
      currentVersion = await getVersion();
      await checkForUpdatesSilently();
      await startApp();
    } catch (error) {
      console.error("[startup] startApp failed", error);
      busy = false;
      status = `Startup failed: ${String(error)}`;
    }
  });

  onDestroy(() => {
    if (resolveConfirm) {
      resolveConfirm(false);
      resolveConfirm = undefined;
    }

    if (unlistenProgress) {
      unlistenProgress();
    }
    if (unlistenComplete) {
      unlistenComplete();
    }
    if (unlistenMenuCheckUpdates) {
      unlistenMenuCheckUpdates();
    }
    if (unlistenMenuRelaunchApp) {
      unlistenMenuRelaunchApp();
    }
    if (debugPollingTimer) {
      clearInterval(debugPollingTimer);
    }
  });
</script>

{#if isDebugWindow}
  <main class="shell">
    <section class="app-card">
      <h2>Debug Logs</h2>
      <pre class="log-view">{debugLogs.join("\n")}</pre>
    </section>
  </main>
{:else}
  <main class="shell">
  <section class="app-card">
    <img class="logo" src={logo} alt="Pullyt" />

    <div class="provider">YouTube</div>

    <input class="url-input" type="text" bind:value={url} placeholder="Paste YouTube URL" />

    <div class="row radios">
      {#each modeOptions as option}
        <label><input type="radio" bind:group={mode} value={option.value} /> {option.label}</label>
      {/each}
    </div>

    {#if mode === "video_with_audio"}
      <div class="row radios">
        <span class="field">Preset</span>
        {#each presetOptions as option}
          <label><input type="radio" bind:group={preset} value={option.value} /> {option.label}</label>
        {/each}
      </div>
    {/if}

    <div class="row radios">
      <span class="field">Video</span>
      {#each videoOptions as option}
        <label><input type="radio" bind:group={videoQuality} value={option.value} /> {option.label}</label>
      {/each}
    </div>

    <div class="row radios">
      <span class="field">Audio</span>
      {#each audioOptions as option}
        <label><input type="radio" bind:group={audioQuality} value={option.value} /> {option.label}</label>
      {/each}
    </div>

    <div class="row">
      <span class="field with-help">
        Cookies
        <button type="button" class="help" aria-label="Cookies help">
          i
          <span class="tooltip">
            Optional. Use this when a video is age-restricted or asks for sign-in. It lets yt-dlp reuse your browser session to access protected videos. Many public videos work without it.
          </span>
        </button>
      </span>
      <select bind:value={cookiesFromBrowser}>
        {#each cookiesBrowserOptions as option}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </div>

    <p class="status">{status}</p>

    <progress max="1" value={progress}></progress>

    <button class="primary" disabled={busy} on:click={onDownload}>{busy ? "Working..." : "Download"}</button>

    <footer>
      <pre>{dependencyInfo}</pre>
      <div class="meta-row">
        <span class="version">v{currentVersion || "1.0.0"}</span>
        <div class="dev">
          <span>developed by Pau Segarra</span>
          <button class="link" on:click={onOpenGithub}>Github</button>
        </div>
      </div>
    </footer>
  </section>
</main>
{/if}

{#if infoDialog.open}
  <div class="confirm-overlay" role="presentation">
    <section class="confirm-modal" role="dialog" aria-modal="true" aria-label={infoDialog.title}>
      <h2>{infoDialog.title}</h2>
      <p>{infoDialog.message}</p>
      <div class="confirm-actions">
        <button class="primary" on:click={closeInfoDialog}>Got it</button>
      </div>
    </section>
  </div>
{/if}

{#if confirmDialog.open}
  <div class="confirm-overlay" role="presentation">
    <section class="confirm-modal" role="dialog" aria-modal="true" aria-label={confirmDialog.title}>
      <h2>{confirmDialog.title}</h2>
      <p>{confirmDialog.message}</p>
      <div class="confirm-actions">
        <button on:click={() => onConfirmDialog(false)}>{confirmDialog.cancelLabel}</button>
        <button class="primary" on:click={() => onConfirmDialog(true)}>{confirmDialog.confirmLabel}</button>
      </div>
    </section>
  </div>
{/if}
