import { invoke } from "@tauri-apps/api/core";
import { AUDIO_CODECS } from "$lib/stores/encode.svelte";

export type UiDensity = "comfortable" | "compact";
export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

// Mirrors src-tauri/src/config.rs's `Config` wire shape. `ffmpegPath`/
// `ffprobePath` are carried through untouched — `tools.svelte.ts` owns
// those two fields via its own dedicated `set_tool_path` flow.
export interface Config {
  ffmpegPath: string | null;
  ffprobePath: string | null;
  outputNamingTemplate: string;
  disabledAudioCodecs: string[];
  uiDensity: UiDensity;
  language: string;
  checkForUpdates: boolean;
  logLevel: LogLevel;
}

// Mirrors src-tauri/src/naming.rs's `NamingVariableInfo` wire shape.
export interface NamingVariableInfo {
  name: string;
  description: string;
  example: string;
  postEncode: boolean;
}

class ConfigState {
  data: Config | null = $state(null);
  namingVariables: NamingVariableInfo[] = $state([]);
  namingPreview = $state("");

  async refresh(): Promise<void> {
    const [data, namingVariables] = await Promise.all([
      invoke<Config>("get_config"),
      invoke<NamingVariableInfo[]>("list_naming_variables"),
    ]);
    this.data = data;
    this.namingVariables = namingVariables;
    await this.refreshPreview();
  }

  /** Persists whatever's currently in `data` — callers mutate `data.*`
   * directly (it's plain `$state`, same pattern as `encode.settings`) then
   * call this on `onchange`/`oninput`, rather than routing every field
   * through its own setter method. */
  async persist(): Promise<void> {
    if (!this.data) {
      return;
    }
    await invoke("update_config", { config: $state.snapshot(this.data) });
  }

  async refreshPreview(): Promise<void> {
    if (!this.data) {
      return;
    }
    this.namingPreview = await invoke<string>("preview_naming_template", {
      template: this.data.outputNamingTemplate,
    });
  }

  isAudioCodecEnabled(codecId: string): boolean {
    return !(this.data?.disabledAudioCodecs.includes(codecId) ?? false);
  }

  /** The codec picker options to actually offer — curated by Settings'
   * audio-encoder list, but always including `alreadySelected` (if given)
   * even when it's since been disabled, so an existing selection never
   * ends up bound to a value with no matching `<option>` (the same class
   * of bug the downmix `<select>` had — notes/plan.md M10's scope note). */
  availableAudioCodecs(alreadySelected?: string): typeof AUDIO_CODECS {
    return AUDIO_CODECS.filter(
      (codec) =>
        this.isAudioCodecEnabled(codec.id) || codec.id === alreadySelected,
    );
  }

  async toggleAudioCodec(codecId: string, enabled: boolean): Promise<void> {
    if (!this.data) {
      return;
    }
    this.data.disabledAudioCodecs = enabled
      ? this.data.disabledAudioCodecs.filter((id) => id !== codecId)
      : [...this.data.disabledAudioCodecs, codecId];
    await this.persist();
  }
}

export const config = new ConfigState();
