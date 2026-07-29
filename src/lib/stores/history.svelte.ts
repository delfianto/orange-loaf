import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { FilterSpec } from "$lib/filters";
import type { EncoderSettings } from "$lib/stores/encode.svelte";

// Mirrors src-tauri/src/history.rs's `HistoryStatus` wire shape — no
// "running"/"cancelled" variants exist here (unlike Queue's JobStatus)
// since only finished jobs are ever recorded in the first place.
export type HistoryStatus =
  { state: "completed" } | { state: "failed"; message: string };

export interface HistoryEntry {
  id: string;
  source: string;
  output: string;
  settings: EncoderSettings;
  filters: FilterSpec;
  completedAtUnixMs: number;
  status: HistoryStatus;
  thumbnail: string | null;
}

interface HistorySnapshot {
  entries: HistoryEntry[];
}

export interface AppliedHistoryEntry {
  settings: EncoderSettings;
  filters: FilterSpec;
}

class HistoryState {
  entries: HistoryEntry[] = $state([]);

  private listening: Promise<void> | null = null;

  private async ensureListening(): Promise<void> {
    this.listening ??= listen<HistorySnapshot>("history:snapshot", (event) => {
      this.entries = event.payload.entries;
    }).then(() => undefined);
    await this.listening;
  }

  async refresh(): Promise<void> {
    await this.ensureListening();
    const snapshot = await invoke<HistorySnapshot>("list_history");
    this.entries = snapshot.entries;
  }

  async remove(id: string): Promise<void> {
    const snapshot = await invoke<HistorySnapshot>("delete_history_entry", {
      id,
    });
    this.entries = snapshot.entries;
  }

  async clear(): Promise<void> {
    const snapshot = await invoke<HistorySnapshot>("clear_history");
    this.entries = snapshot.entries;
  }

  /** Only encoder settings + filters transfer — see
   * `apply_history_entry`'s doc comment in history.rs for why trim/track
   * selections deliberately don't. */
  async apply(id: string): Promise<AppliedHistoryEntry> {
    return invoke<AppliedHistoryEntry>("apply_history_entry", { id });
  }
}

export const history = new HistoryState();
