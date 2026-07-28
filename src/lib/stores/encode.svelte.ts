import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const X265_PRESETS = [
  "ultrafast",
  "superfast",
  "veryfast",
  "faster",
  "fast",
  "medium",
  "slow",
  "slower",
  "veryslow",
  "placebo",
] as const;

export interface X265Settings {
  preset: string;
  tune: string | null;
  profile: string | null;
  crf: number;
  pixFmt: string;
}

export interface Crop {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

export interface FilterSpec {
  crop: Crop | null;
  scaleWidth: number | null;
  scaleHeight: number | null;
  rotateDegrees: number;
  horizontalFlip: boolean;
  verticalFlip: boolean;
}

export type EncodeStatus =
  | { state: "running" }
  | { state: "completed" }
  | { state: "failed"; message: string };

export interface EncodeProgress {
  outTimeSecs: number;
  percent: number | null;
  speed: number | null;
  fps: number | null;
}

interface EncodeEvent {
  jobId: string;
  status: EncodeStatus;
  progress: EncodeProgress | null;
}

function defaultSettings(): X265Settings {
  return {
    preset: "medium",
    tune: null,
    profile: null,
    crf: 22,
    pixFmt: "yuv420p10le",
  };
}

function defaultFilters(): FilterSpec {
  return {
    crop: null,
    scaleWidth: null,
    scaleHeight: null,
    rotateDegrees: 0,
    horizontalFlip: false,
    verticalFlip: false,
  };
}

class EncodeState {
  settings: X265Settings = $state(defaultSettings());
  filters: FilterSpec = $state(defaultFilters());
  commandPreview: string[] = $state([]);
  jobId: string | null = $state(null);
  status: EncodeStatus | null = $state(null);
  progress: EncodeProgress | null = $state(null);
  starting = $state(false);

  private listening: Promise<void> | null = null;

  private async ensureListening(): Promise<void> {
    this.listening ??= listen<EncodeEvent>("encode:event", (event) => {
      if (event.payload.jobId !== this.jobId) {
        return;
      }
      this.status = event.payload.status;
      this.progress = event.payload.progress;
    }).then(() => undefined);
    await this.listening;
  }

  async refreshPreview(
    path: string | null,
    settings: X265Settings,
    filters: FilterSpec,
  ): Promise<void> {
    if (!path) {
      this.commandPreview = [];
      return;
    }
    try {
      this.commandPreview = await invoke<string[]>("preview_command", {
        path,
        settings,
        filters,
      });
    } catch {
      this.commandPreview = [];
    }
  }

  async start(path: string, durationSecs: number): Promise<void> {
    await this.ensureListening();
    this.starting = true;
    this.status = null;
    this.progress = null;
    try {
      this.jobId = await invoke<string>("start_encode", {
        path,
        durationSecs,
        settings: this.settings,
        filters: this.filters,
      });
    } catch (err) {
      this.status = { state: "failed", message: String(err) };
    } finally {
      this.starting = false;
    }
  }
}

export const encode = new EncodeState();
