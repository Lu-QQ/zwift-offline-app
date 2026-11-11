import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface Settings {
  engine_path: string | null;
  engine_exe: string | null;
  zwift_dir: string | null;
  strava_port: number;
}

export interface CheckItem {
  id: string;
  name: string;
  status: "ok" | "warn" | "fail" | "info";
  detail: string;
  hint: string;
}

export interface StepResult {
  id: string;
  name: string;
  status: "done" | "skipped" | "error" | "log";
  detail: string;
}

export interface ServiceStatus {
  running: boolean;
  pid: number | null;
  healthy: boolean;
}

export interface StravaStatus {
  configured: boolean;
  token_file: string | null;
  expires_at: number | null;
  expired: boolean;
  player_id: string | null;
}

export interface ZwiftSyncStatus {
  configured: boolean;
  username: string | null;
  player_id: string | null;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (s: Settings) => invoke<void>("save_settings", { settings: s }),
  detect: () => invoke<CheckItem[]>("detect_run"),
  configApply: () => invoke<StepResult[]>("config_apply"),
  restoreRun: () => invoke<StepResult[]>("restore_run"),
  installDeps: () => invoke<string>("install_deps"),
  installPython: () => invoke<string>("install_python"),
  openUrl: (url: string) => openUrl(url),
  serviceStart: () => invoke<string>("service_start"),
  serviceStop: () => invoke<string>("service_stop"),
  serviceStatus: () => invoke<ServiceStatus>("service_status"),
  serviceLogs: () => invoke<string[]>("service_logs"),
  launchZwift: () => invoke<string>("launch_zwift"),
  stravaStatus: () => invoke<StravaStatus>("strava_status"),
  stravaAuthorize: (clientId: string, clientSecret: string) =>
    invoke<string>("strava_authorize", { clientId, clientSecret }),
  zwiftSyncStatus: () => invoke<ZwiftSyncStatus>("zwift_sync_status"),
  zwiftSyncSave: (username: string, password: string, verify: boolean) =>
    invoke<string>("zwift_sync_save", { username, password, verify }),
  zwiftSyncClear: () => invoke<string>("zwift_sync_clear"),
  pickDir: () => open({ directory: true, multiple: false }) as Promise<string | null>,
  onServiceLog: (cb: (line: string) => void): Promise<UnlistenFn> =>
    listen<string>("service:log", (e) => cb(e.payload)),
  onServiceExited: (cb: (msg: string) => void): Promise<UnlistenFn> =>
    listen<string>("service:exited", (e) => cb(e.payload)),
  onConfigProgress: (cb: (r: StepResult) => void): Promise<UnlistenFn> =>
    listen<StepResult>("config:progress", (e) => cb(e.payload)),
  checkUpdate: () => check(),
  installUpdate: async (u: Update, onProgress?: (pct: number) => void) => {
    let total = 0;
    let done = 0;
    await u.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          total = event.data.contentLength ?? 0;
          break;
        case "Progress":
          done += event.data.chunkLength ?? 0;
          if (total > 0) onProgress?.(Math.min(99, Math.round((done / total) * 100)));
          break;
        case "Finished":
          onProgress?.(100);
          break;
      }
    });
    await relaunch();
  },
};
