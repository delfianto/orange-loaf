import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { defaultFilters, type FilterSpec } from "$lib/filters";
import type {
  AudioTrack,
  SubtitleTrack,
  VideoInfo,
} from "$lib/stores/video.svelte";

export type { Crop, FilterSpec } from "$lib/filters";

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

// Scoped to codecs actually verified against this project's ffmpeg build
// (notes/plan.md M7 scope note) — Appendix A's full table includes several
// (libvorbis, libfdk_aac) that aren't reliably compiled into stock ffmpeg
// and so aren't offered here.
export const AUDIO_CODECS: { id: string; label: string; lossless: boolean }[] =
  [
    { id: "aac", label: "AAC", lossless: false },
    { id: "libopus", label: "Opus", lossless: false },
    { id: "libmp3lame", label: "MP3", lossless: false },
    { id: "ac3", label: "AC-3 (Dolby Digital)", lossless: false },
    { id: "flac", label: "FLAC (lossless)", lossless: true },
  ];

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

interface VideoToolboxFields {
  quality: number;
}

// Matches the exact wire shape of the Rust `#[serde(tag = "name")]` enum:
// the tag and the inner struct's fields flattened at the same level (pinned
// by a Rust-side test in encoders/mod.rs, not assumed independently here).
export type EncoderSettings =
  | ({ name: "x265" } & X265Fields)
  | ({ name: "x264" } & X264Fields)
  | ({ name: "svtAv1" } & SvtAv1Fields)
  | ({ name: "h264Videotoolbox" } & VideoToolboxFields)
  | ({ name: "hevcVideotoolbox" } & VideoToolboxFields);

export type EncoderName = EncoderSettings["name"];

export const ENCODER_NAMES: { id: EncoderName; label: string }[] = [
  { id: "x265", label: "HEVC (x265)" },
  { id: "x264", label: "AVC (x264)" },
  { id: "svtAv1", label: "AV1 (SVT-AV1)" },
  { id: "h264Videotoolbox", label: "H.264 (VideoToolbox)" },
  { id: "hevcVideotoolbox", label: "HEVC (VideoToolbox)" },
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
    case "h264Videotoolbox":
      return { name: "h264Videotoolbox", quality: 60 };
    case "hevcVideotoolbox":
      return { name: "hevcVideotoolbox", quality: 60 };
  }
}

// Mirrors src-tauri/src/encoders/mod.rs's `Trim` wire shape.
export interface Trim {
  startTime: number | null;
  endTime: number | null;
  fastSeek: boolean;
}

export function defaultTrim(): Trim {
  return { startTime: null, endTime: null, fastSeek: true };
}

// Mirrors src-tauri/src/encoders/tracks.rs's `AudioAction` wire shape
// (pinned by a Rust-side test there, same reasoning as `EncoderSettings`).
export type AudioAction =
  | { action: "copy" }
  | {
      action: "convert";
      codec: string;
      quality: number;
      downmix: number | null;
    };

export interface AudioTrackSelection {
  index: number;
  sourceChannels: number;
  enabled: boolean;
  action: AudioAction;
}

export interface SubtitleTrackSelection {
  index: number;
  enabled: boolean;
}

function selectionFromAudioTrack(track: AudioTrack): AudioTrackSelection {
  return {
    index: track.index,
    sourceChannels: track.channels,
    enabled: true,
    action: { action: "copy" },
  };
}

function selectionFromSubtitleTrack(
  track: SubtitleTrack,
): SubtitleTrackSelection {
  return { index: track.index, enabled: true };
}

// Mirrors src-tauri/src/job.rs's `EncodeStatus` wire shape exactly, including
// "cancelled" — unreachable from this direct-encode path today (there's no
// cancel button here yet), but genuinely reachable once a job runs through
// the queue (src-tauri/src/queue.rs), which reuses this same event shape.
export type EncodeStatus =
  | { state: "running" }
  | { state: "completed" }
  | { state: "failed"; message: string }
  | { state: "cancelled" };

export interface EncodeProgress {
  outTimeSecs: number;
  percent: number | null;
  speed: number | null;
  fps: number | null;
}

// Exported so `queue.svelte.ts` can listen to the same `encode:event` for
// whichever job the queue is currently running, instead of redeclaring an
// identical shape.
export interface EncodeEvent {
  jobId: string;
  status: EncodeStatus;
  progress: EncodeProgress | null;
}

class EncodeState {
  settings: EncoderSettings = $state(defaultSettingsFor("x265"));
  filters: FilterSpec = $state(defaultFilters());
  trim: Trim = $state(defaultTrim());
  audioTracks: AudioTrackSelection[] = $state([]);
  subtitleTracks: SubtitleTrackSelection[] = $state([]);
  commandPreview: string[] = $state([]);
  jobId: string | null = $state(null);
  status: EncodeStatus | null = $state(null);
  progress: EncodeProgress | null = $state(null);
  starting = $state(false);

  private listening: Promise<void> | null = null;

  selectEncoder(name: EncoderName): void {
    this.settings = defaultSettingsFor(name);
  }

  /**
   * Re-derives the track selection from a freshly-probed source, every
   * track defaulting to enabled+copy — call whenever `video.info` changes
   * (a new source loaded, or cleared). Previous per-track choices are
   * deliberately not preserved across sources: track *count* and codec can
   * differ entirely between files, so index-based carryover would silently
   * apply stale settings to unrelated tracks.
   */
  resetTracksFrom(info: VideoInfo | null): void {
    this.audioTracks = info?.audioTracks.map(selectionFromAudioTrack) ?? [];
    this.subtitleTracks =
      info?.subtitleTracks.map(selectionFromSubtitleTrack) ?? [];
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
    trim: Trim,
    audioTracks: AudioTrackSelection[],
    subtitleTracks: SubtitleTrackSelection[],
    videoInfo: VideoInfo | null,
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
        trim,
        audioTracks,
        subtitleTracks,
        videoInfo,
      });
    } catch {
      this.commandPreview = [];
    }
  }

  // `videoInfo` isn't user-editable state, so it's threaded through as a
  // plain parameter sourced straight from the `video` store rather than
  // duplicated into its own field here — it feeds both HDR passthrough
  // (M8) and the naming-template system's source/color/audio variables
  // (src-tauri/src/naming.rs, M12).
  async start(
    path: string,
    durationSecs: number,
    videoInfo: VideoInfo | null,
  ): Promise<void> {
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
        trim: this.trim,
        audioTracks: this.audioTracks,
        subtitleTracks: this.subtitleTracks,
        videoInfo,
      });
    } catch (err) {
      this.status = { state: "failed", message: String(err) };
    } finally {
      this.starting = false;
    }
  }
}

export const encode = new EncodeState();
