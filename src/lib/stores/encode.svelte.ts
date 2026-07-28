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

export const X264_PRESETS = X265_PRESETS;

interface X265Fields {
  preset: string;
  tune: string | null;
  profile: string | null;
  crf: number;
  pixFmt: string;
}

interface X264Fields {
  preset: string;
  tune: string | null;
  profile: string | null;
  crf: number;
}

interface SvtAv1Fields {
  speed: number;
  crf: number;
  tune: number | null;
}

// Matches the exact wire shape of the Rust `#[serde(tag = "name")]` enum:
// the tag and the inner struct's fields flattened at the same level (pinned
// by a Rust-side test in encoders/mod.rs, not assumed independently here).
export type EncoderSettings =
  | ({ name: "x265" } & X265Fields)
  | ({ name: "x264" } & X264Fields)
  | ({ name: "svtAv1" } & SvtAv1Fields);

export type EncoderName = EncoderSettings["name"];

export const ENCODER_NAMES: { id: EncoderName; label: string }[] = [
  { id: "x265", label: "HEVC (x265)" },
  { id: "x264", label: "AVC (x264)" },
  { id: "svtAv1", label: "AV1 (SVT-AV1)" },
];

export function defaultSettingsFor(name: EncoderName): EncoderSettings {
  switch (name) {
    case "x265":
      return {
        name: "x265",
        preset: "medium",
        tune: null,
        profile: null,
        crf: 22,
        pixFmt: "yuv420p10le",
      };
    case "x264":
      return {
        name: "x264",
        preset: "medium",
        tune: null,
        profile: null,
        crf: 23,
      };
    case "svtAv1":
      return { name: "svtAv1", speed: 7, crf: 24, tune: null };
  }
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
  settings: EncoderSettings = $state(defaultSettingsFor("x265"));
  filters: FilterSpec = $state(defaultFilters());
  commandPreview: string[] = $state([]);
  jobId: string | null = $state(null);
  status: EncodeStatus | null = $state(null);
  progress: EncodeProgress | null = $state(null);
  starting = $state(false);

  private listening: Promise<void> | null = null;

  selectEncoder(name: EncoderName): void {
    this.settings = defaultSettingsFor(name);
  }

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
    settings: EncoderSettings,
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
