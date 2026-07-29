import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface ToolStatus {
  name: string;
  found: boolean;
  path: string | null;
  version: string | null;
}

class ToolsState {
  statuses: ToolStatus[] = $state([]);
  /** `null` until the first real check resolves — distinct from `false`
   * (checked, and genuinely unavailable), so the status bar can show
   * "checking…" rather than a premature "not available". */
  nvencAvailable: boolean | null = $state(null);
  loading = $state(false);

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      const [statuses, nvencAvailable] = await Promise.all([
        invoke<ToolStatus[]>("detect_tools"),
        invoke<boolean>("detect_nvenc"),
      ]);
      this.statuses = statuses;
      this.nvencAvailable = nvencAvailable;
    } finally {
      this.loading = false;
    }
  }

  async browseFor(toolKey: "ffmpeg" | "ffprobe"): Promise<void> {
    const selected = await open({
      multiple: false,
      directory: false,
      title: `Choose the ${toolKey} binary`,
    });
    if (typeof selected !== "string") {
      return;
    }
    this.statuses = await invoke<ToolStatus[]>("set_tool_path", {
      tool: toolKey,
      path: selected,
    });
  }
}

export const tools = new ToolsState();
