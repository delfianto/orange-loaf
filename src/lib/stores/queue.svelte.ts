import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { FilterSpec } from "$lib/filters";
import type {
  AudioTrackSelection,
  EncodeEvent,
  EncodeProgress,
  EncoderSettings,
  SubtitleTrackSelection,
  Trim,
} from "$lib/stores/encode.svelte";
import type { VideoInfo } from "$lib/stores/video.svelte";

// Mirrors src-tauri/src/queue.rs's `JobStatus` wire shape.
export type JobStatus =
  | { state: "queued" }
  | { state: "running" }
  | { state: "completed" }
  | { state: "failed"; message: string }
  | { state: "cancelled" };

// Mirrors src-tauri/src/queue.rs's `Job` wire shape. Reuses the Convert
// screen's own settings/filters/trim/track/HDR types directly, since a
// queued job is exactly "everything `start_encode` takes," persisted.
export interface Job {
  id: string;
  source: string;
  output: string;
  durationSecs: number;
  settings: EncoderSettings;
  filters: FilterSpec;
  trim: Trim;
  audioTracks: AudioTrackSelection[];
  subtitleTracks: SubtitleTrackSelection[];
  videoInfo: VideoInfo | null;
  status: JobStatus;
}

interface QueueSnapshot {
  jobs: Job[];
  paused: boolean;
}

interface LogEvent {
  jobId: string;
  line: string;
}

// Bounds each job's in-memory log buffer — this is a live tail for whatever
// is currently running, not a durable transcript (nothing here survives a
// restart, and a finished job's buffer is only what happened to still be in
// memory when it finished).
const MAX_LOG_LINES = 500;

class QueueState {
  jobs: Job[] = $state([]);
  paused = $state(false);
  /** Live progress for whichever job is currently running, keyed by job id
   * — sourced from `encode:event` (the same event the direct-encode path
   * uses), not `queue:snapshot`, which only changes on status transitions. */
  progress: Record<string, EncodeProgress> = $state({});
  /** Live stderr tail per job, from `job:log` — see `MAX_LOG_LINES`. */
  logs: Record<string, string[]> = $state({});

  private listening: Promise<void> | null = null;

  private applySnapshot(snapshot: QueueSnapshot): void {
    this.jobs = snapshot.jobs;
    this.paused = snapshot.paused;
  }

  private async ensureListening(): Promise<void> {
    this.listening ??= Promise.all([
      listen<QueueSnapshot>("queue:snapshot", (event) => {
        this.applySnapshot(event.payload);
      }),
      listen<EncodeEvent>("encode:event", (event) => {
        if (event.payload.progress) {
          this.progress = {
            ...this.progress,
            [event.payload.jobId]: event.payload.progress,
          };
        }
      }),
      listen<LogEvent>("job:log", (event) => {
        const existing = this.logs[event.payload.jobId] ?? [];
        this.logs = {
          ...this.logs,
          [event.payload.jobId]: [...existing, event.payload.line].slice(
            -MAX_LOG_LINES,
          ),
        };
      }),
    ]).then(() => undefined);
    await this.listening;
  }

  async refresh(): Promise<void> {
    await this.ensureListening();
    this.applySnapshot(await invoke<QueueSnapshot>("list_queue"));
  }

  async enqueue(
    source: string,
    durationSecs: number,
    settings: EncoderSettings,
    filters: FilterSpec,
    trim: Trim,
    audioTracks: AudioTrackSelection[],
    subtitleTracks: SubtitleTrackSelection[],
    videoInfo: VideoInfo | null,
  ): Promise<void> {
    await this.ensureListening();
    this.applySnapshot(
      await invoke<QueueSnapshot>("enqueue_job", {
        source,
        durationSecs,
        settings,
        filters,
        trim,
        audioTracks,
        subtitleTracks,
        videoInfo,
      }),
    );
  }

  /** Re-queues a failed job's exact settings as a new job, then drops the
   * old failed entry — a fresh id/queue position, not an in-place retry. */
  async retry(job: Job): Promise<void> {
    await this.enqueue(
      job.source,
      job.durationSecs,
      job.settings,
      job.filters,
      job.trim,
      job.audioTracks,
      job.subtitleTracks,
      job.videoInfo,
    );
    await this.remove(job.id);
  }

  async remove(jobId: string): Promise<void> {
    this.applySnapshot(
      await invoke<QueueSnapshot>("remove_from_queue", { jobId }),
    );
  }

  async move(jobId: string, direction: "up" | "down"): Promise<void> {
    this.applySnapshot(
      await invoke<QueueSnapshot>("move_job", { jobId, direction }),
    );
  }

  async cancel(jobId: string): Promise<void> {
    this.applySnapshot(await invoke<QueueSnapshot>("cancel_job", { jobId }));
  }

  async pause(): Promise<void> {
    this.applySnapshot(await invoke<QueueSnapshot>("pause_queue"));
  }

  async resume(): Promise<void> {
    this.applySnapshot(await invoke<QueueSnapshot>("resume_queue"));
  }
}

export const queue = new QueueState();
