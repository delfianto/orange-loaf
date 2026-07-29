import { invoke } from "@tauri-apps/api/core";
import type { FilterSpec } from "$lib/filters";
import type {
  AudioAction,
  AudioTrackSelection,
  EncoderSettings,
} from "$lib/stores/encode.svelte";
import type { AudioTrack } from "$lib/stores/video.svelte";

// Mirrors src-tauri/src/profiles.rs's `MatchItem`/`MatchType` wire shape —
// plain unit enums serialize as bare lowercase strings (pinned by a
// Rust-side test there), not `{ "name": ... }`-tagged like `EncoderKind`.
export type MatchItem = "all" | "language" | "channels";
export type MatchType = "all" | "first";

export interface AudioRule {
  matchItem: MatchItem;
  matchInput: string;
  matchType: MatchType;
  action: AudioAction;
}

export function defaultAudioRule(): AudioRule {
  return {
    matchItem: "language",
    matchInput: "",
    matchType: "all",
    action: { action: "copy" },
  };
}

export interface Profile {
  id: string;
  name: string;
  settings: EncoderSettings;
  filters: FilterSpec;
  audioRules: AudioRule[];
}

interface ProfilesSnapshot {
  profiles: Profile[];
}

export interface AppliedProfile {
  settings: EncoderSettings;
  filters: FilterSpec;
  audioTracks: AudioTrackSelection[];
}

class ProfilesState {
  profiles: Profile[] = $state([]);

  async refresh(): Promise<void> {
    const snapshot = await invoke<ProfilesSnapshot>("list_profiles");
    this.profiles = snapshot.profiles;
  }

  async save(
    id: string | null,
    name: string,
    settings: EncoderSettings,
    filters: FilterSpec,
    audioRules: AudioRule[],
  ): Promise<void> {
    const snapshot = await invoke<ProfilesSnapshot>("save_profile", {
      id,
      name,
      settings,
      filters,
      audioRules,
    });
    this.profiles = snapshot.profiles;
  }

  async duplicate(id: string): Promise<void> {
    const snapshot = await invoke<ProfilesSnapshot>("duplicate_profile", {
      id,
    });
    this.profiles = snapshot.profiles;
  }

  async remove(id: string): Promise<void> {
    const snapshot = await invoke<ProfilesSnapshot>("delete_profile", { id });
    this.profiles = snapshot.profiles;
  }

  /** Evaluates `id`'s audio rules against the *currently loaded* source's
   * probed tracks — the caller (Convert screen) is what actually applies
   * the returned settings/filters/track selections to its own state. */
  async apply(
    id: string,
    sourceAudioTracks: AudioTrack[],
  ): Promise<AppliedProfile> {
    return invoke<AppliedProfile>("apply_profile", {
      id,
      sourceAudioTracks,
    });
  }
}

export const profiles = new ProfilesState();
