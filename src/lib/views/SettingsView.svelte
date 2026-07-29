<script lang="ts">
  import { onMount } from "svelte";
  import CircleCheck from "lucide-svelte/icons/circle-check";
  import CircleX from "lucide-svelte/icons/circle-x";
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Loader from "lucide-svelte/icons/loader-circle";
  import RotateCcw from "lucide-svelte/icons/rotate-ccw";
  import { tools } from "$lib/stores/tools.svelte";
  import {
    config,
    type LogLevel,
    type UiDensity,
  } from "$lib/stores/config.svelte";
  import { AUDIO_CODECS } from "$lib/stores/encode.svelte";

  const knownTools = [
    { key: "ffmpeg" as const, displayName: "FFmpeg" },
    { key: "ffprobe" as const, displayName: "FFprobe" },
  ];

  // Must match src-tauri/src/config.rs's `default_naming_template()`.
  const DEFAULT_NAMING_TEMPLATE = "{source}-{encoder}";

  // Labels only — actually retranslating the UI from this value is M13's
  // job (notes/plan.md). Saved now so it's ready once that lands.
  const LANGUAGES = [
    { id: "en", label: "English" },
    { id: "es", label: "Español" },
    { id: "fr", label: "Français" },
    { id: "de", label: "Deutsch" },
    { id: "ja", label: "日本語" },
  ];

  const LOG_LEVELS: { id: LogLevel; label: string }[] = [
    { id: "error", label: "Error" },
    { id: "warn", label: "Warn" },
    { id: "info", label: "Info" },
    { id: "debug", label: "Debug" },
    { id: "trace", label: "Trace" },
  ];

  const sectionHeading = "text-base-content/70 mb-3 text-sm font-medium";
  const card = "bg-base-300 border-hairline rounded-lg border";

  onMount(() => {
    void tools.refresh();
    void config.refresh();
  });

  let templateInputEl: HTMLInputElement | undefined = $state();

  function commitNamingTemplate(): void {
    void config.refreshPreview();
    void config.persist();
  }

  function insertNamingVariable(name: string): void {
    if (!config.data) {
      return;
    }
    const token = `{${name}}`;
    const el = templateInputEl;
    const current = config.data.outputNamingTemplate;
    if (el?.selectionStart != null && el.selectionEnd != null) {
      const { selectionStart: start, selectionEnd: end } = el;
      config.data.outputNamingTemplate =
        current.slice(0, start) + token + current.slice(end);
      const caret = start + token.length;
      requestAnimationFrame(() => {
        el.focus();
        el.setSelectionRange(caret, caret);
      });
    } else {
      config.data.outputNamingTemplate = current + token;
    }
    commitNamingTemplate();
  }

  function resetNamingTemplate(): void {
    if (!config.data) {
      return;
    }
    config.data.outputNamingTemplate = DEFAULT_NAMING_TEMPLATE;
    commitNamingTemplate();
  }

  function onToggleCodec(
    codecId: string,
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    void config.toggleAudioCodec(codecId, event.currentTarget.checked);
  }

  function setDensity(density: UiDensity): void {
    if (!config.data) {
      return;
    }
    config.data.uiDensity = density;
    void config.persist();
  }

  function onLanguageChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    if (!config.data) {
      return;
    }
    config.data.language = event.currentTarget.value;
    void config.persist();
  }

  function onLogLevelChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    if (!config.data) {
      return;
    }
    config.data.logLevel = event.currentTarget.value as LogLevel;
    void config.persist();
  }

  function onCheckForUpdatesChange(
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    if (!config.data) {
      return;
    }
    config.data.checkForUpdates = event.currentTarget.checked;
    void config.persist();
  }
</script>

<div class="flex h-full flex-col">
  <header class="border-hairline flex h-14 shrink-0 items-center border-b px-6">
    <h1 class="text-lg font-semibold">Settings</h1>
  </header>

  <div class="flex-1 overflow-y-auto p-6">
    <div class="mx-auto flex max-w-2xl flex-col gap-6">
      <section>
        <h2 class={sectionHeading}>Detected External Programs</h2>
        <div class="{card} divide-hairline divide-y">
          {#each knownTools as known (known.key)}
            {@const status = tools.statuses.find(
              (s) => s.name === known.displayName,
            )}
            <div class="flex items-center gap-3 px-4 py-3">
              {#if tools.loading && !status}
                <Loader
                  class="text-base-content/40 h-4 w-4 shrink-0 animate-spin"
                />
              {:else if status?.found}
                <CircleCheck class="text-success h-4 w-4 shrink-0" />
              {:else}
                <CircleX class="text-error h-4 w-4 shrink-0" />
              {/if}

              <div class="min-w-0 flex-1">
                <p class="text-sm font-medium">{known.displayName}</p>
                {#if status?.found}
                  <p class="text-base-content/50 truncate font-mono text-xs">
                    {status.path}
                  </p>
                  {#if status.version}
                    <p class="text-base-content/40 truncate text-xs">
                      {status.version}
                    </p>
                  {/if}
                {:else}
                  <p class="text-base-content/50 text-xs">Not found on PATH</p>
                {/if}
              </div>

              <button
                type="button"
                class="btn btn-ghost btn-xs"
                onclick={() => tools.browseFor(known.key)}
              >
                <FolderOpen class="h-3.5 w-3.5" />
                Browse
              </button>
            </div>
          {/each}
        </div>
      </section>

      <section>
        <h2 class={sectionHeading}>Output Naming</h2>
        <div class="{card} p-4">
          {#if config.data}
            <label
              class="text-base-content/50 mb-1.5 block text-xs"
              for="naming-template"
            >
              Naming Template
            </label>
            <input
              id="naming-template"
              bind:this={templateInputEl}
              class="input input-sm w-full font-mono"
              bind:value={config.data.outputNamingTemplate}
              oninput={() => void config.refreshPreview()}
              onchange={commitNamingTemplate}
            />
            <div class="mt-2 flex flex-wrap gap-1.5">
              {#each config.namingVariables as variable (variable.name)}
                <button
                  type="button"
                  class="badge badge-sm cursor-pointer {variable.postEncode
                    ? 'badge-outline'
                    : 'badge-ghost'}"
                  title="{variable.description} (e.g. {variable.example})"
                  onclick={() => {
                    insertNamingVariable(variable.name);
                  }}
                >
                  {"{" + variable.name + "}"}
                </button>
              {/each}
            </div>
            <div
              class="border-hairline mt-3 flex items-center gap-2 border-t pt-3"
            >
              <span
                class="text-base-content/50 shrink-0 text-[10px] font-semibold tracking-wider uppercase"
              >
                Preview
              </span>
              <span class="truncate font-mono text-xs">
                {config.namingPreview || "—"}.mkv
              </span>
            </div>
            <button
              type="button"
              class="btn btn-ghost btn-xs mt-3"
              onclick={resetNamingTemplate}
            >
              <RotateCcw class="h-3 w-3" />
              Reset to Default
            </button>
          {:else}
            <Loader class="text-base-content/40 h-4 w-4 animate-spin" />
          {/if}
        </div>
      </section>

      <section>
        <h2 class={sectionHeading}>Audio Encoders</h2>
        <div class="{card} divide-hairline divide-y">
          {#each AUDIO_CODECS as codec (codec.id)}
            <label class="flex items-center gap-3 px-4 py-2.5">
              <input
                type="checkbox"
                class="checkbox checkbox-sm"
                checked={config.isAudioCodecEnabled(codec.id)}
                onchange={(e) => {
                  onToggleCodec(codec.id, e);
                }}
              />
              <span class="flex-1 text-sm">{codec.label}</span>
              {#if codec.lossless}
                <span class="badge badge-xs badge-outline">lossless</span>
              {/if}
            </label>
          {/each}
        </div>
        <p class="text-base-content/50 mt-2 text-xs">
          Unchecked codecs are hidden from the audio-conversion pickers on the
          Convert and Profiles screens.
        </p>
      </section>

      <section>
        <h2 class={sectionHeading}>Appearance</h2>
        <div class="{card} flex items-center justify-between p-4">
          <span class="text-sm">UI Density</span>
          <div class="join">
            <button
              type="button"
              class="join-item btn btn-xs {config.data?.uiDensity ===
              'comfortable'
                ? 'btn-primary'
                : 'btn-ghost'}"
              onclick={() => {
                setDensity("comfortable");
              }}
            >
              Comfortable
            </button>
            <button
              type="button"
              class="join-item btn btn-xs {config.data?.uiDensity === 'compact'
                ? 'btn-primary'
                : 'btn-ghost'}"
              onclick={() => {
                setDensity("compact");
              }}
            >
              Compact
            </button>
          </div>
        </div>
      </section>

      <section>
        <h2 class={sectionHeading}>Language</h2>
        <div class="{card} flex items-center justify-between p-4">
          <span class="text-sm">Display Language</span>
          <select
            class="select select-sm"
            value={config.data?.language}
            onchange={onLanguageChange}
          >
            {#each LANGUAGES as lang (lang.id)}
              <option value={lang.id}>{lang.label}</option>
            {/each}
          </select>
        </div>
        <p class="text-base-content/50 mt-2 text-xs">
          Retranslating the interface from this setting lands in a later
          milestone — it's saved now so it's ready once that's wired up.
        </p>
      </section>

      <section>
        <h2 class={sectionHeading}>Behavior</h2>
        <div class="{card} flex items-center justify-between p-4">
          <span class="text-sm">Check for updates automatically</span>
          <input
            type="checkbox"
            class="toggle toggle-sm"
            checked={config.data?.checkForUpdates ?? false}
            onchange={onCheckForUpdatesChange}
          />
        </div>
      </section>

      <section>
        <h2 class={sectionHeading}>Logging</h2>
        <div class="{card} flex items-center justify-between p-4">
          <span class="text-sm">Log Level</span>
          <select
            class="select select-sm"
            value={config.data?.logLevel}
            onchange={onLogLevelChange}
          >
            {#each LOG_LEVELS as level (level.id)}
              <option value={level.id}>{level.label}</option>
            {/each}
          </select>
        </div>
      </section>
    </div>
  </div>
</div>
