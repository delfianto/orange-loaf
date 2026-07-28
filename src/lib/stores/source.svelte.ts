import { open } from "@tauri-apps/plugin-dialog";

class SourceState {
  path: string | null = $state(null);

  async choose(): Promise<void> {
    const selected = await open({
      multiple: false,
      directory: false,
      title: "Choose a video file",
    });
    if (typeof selected === "string") {
      this.path = selected;
    }
  }

  clear(): void {
    this.path = null;
  }
}

export const source = new SourceState();
