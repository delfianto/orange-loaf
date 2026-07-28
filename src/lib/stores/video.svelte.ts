import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface VideoInfo {
  durationSecs: number;
  width: number;
  height: number;
  codecName: string;
  pixFmt: string | null;
  bitDepth: number;
  frameRate: number | null;
  hdr10: boolean;
}

class VideoState {
  path: string | null = $state(null);
  info: VideoInfo | null = $state(null);
  thumbnail: string | null = $state(null);
  scrubTime = $state(0);
  probing = $state(false);
  thumbnailLoading = $state(false);
  error: string | null = $state(null);

  async choose(): Promise<void> {
    const selected = await open({
      multiple: false,
      directory: false,
      title: "Choose a video file",
    });
    if (typeof selected === "string") {
      await this.loadFrom(selected);
    }
  }

  async loadFrom(path: string): Promise<void> {
    this.path = path;
    this.info = null;
    this.thumbnail = null;
    this.scrubTime = 0;
    this.error = null;
    this.probing = true;
    try {
      this.info = await invoke<VideoInfo>("probe_video", { path });
      await this.refreshThumbnail();
    } catch (err) {
      this.error = String(err);
    } finally {
      this.probing = false;
    }
  }

  async seekTo(seconds: number): Promise<void> {
    this.scrubTime = seconds;
    await this.refreshThumbnail();
  }

  private async refreshThumbnail(): Promise<void> {
    if (!this.path) {
      return;
    }
    this.thumbnailLoading = true;
    try {
      this.thumbnail = await invoke<string>("generate_thumbnail", {
        path: this.path,
        timestampSecs: this.scrubTime,
      });
    } catch (err) {
      this.error = String(err);
    } finally {
      this.thumbnailLoading = false;
    }
  }

  clear(): void {
    this.path = null;
    this.info = null;
    this.thumbnail = null;
    this.scrubTime = 0;
    this.error = null;
  }
}

export const video = new VideoState();
