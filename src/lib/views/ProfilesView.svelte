<script lang="ts">
  import { onMount } from "svelte";
  import Layers from "lucide-svelte/icons/layers";
  import Plus from "lucide-svelte/icons/plus";
  import Copy from "lucide-svelte/icons/copy";
  import Trash2 from "lucide-svelte/icons/trash-2";
  import Play from "lucide-svelte/icons/play";
  import ChevronDown from "lucide-svelte/icons/chevron-down";
  import ChevronRight from "lucide-svelte/icons/chevron-right";
  import {
    profiles,
    defaultAudioRule,
    type Profile,
    type AudioRule,
  } from "$lib/stores/profiles.svelte";
  import {
    encode,
    ENCODER_NAMES,
    AUDIO_CODECS,
    type AudioAction,
  } from "$lib/stores/encode.svelte";
  import { video } from "$lib/stores/video.svelte";
  import { navigation } from "$lib/stores/navigation.svelte";

  onMount(() => {
    void profiles.refresh();
  });

  let expandedId: string | null = $state(null);
  let editingId: string | null = $state(null);
  let editingName = $state("");

  function encoderLabel(name: string): string {
    return ENCODER_NAMES.find((e) => e.id === name)?.label ?? name;
  }

  async function createFromCurrentSettings(): Promise<void> {
    await profiles.save(
      null,
      "New Profile",
      $state.snapshot(encode.settings),
      $state.snapshot(encode.filters),
      [],
    );
  }

  function startRename(profile: Profile): void {
    editingId = profile.id;
    editingName = profile.name;
  }

  function commitRename(profile: Profile): void {
    editingId = null;
    const name = editingName.trim();
    if (name && name !== profile.name) {
      void profiles.save(
        profile.id,
        name,
        profile.settings,
        profile.filters,
        profile.audioRules,
      );
    }
  }

  async function applyProfile(profile: Profile): Promise<void> {
    const sourceAudioTracks = video.info?.audioTracks ?? [];
    const applied = await profiles.apply(profile.id, sourceAudioTracks);
    encode.settings = applied.settings;
    encode.filters = applied.filters;
    encode.audioTracks = applied.audioTracks;
    navigation.current = "convert";
  }

  function updateRules(profile: Profile, rules: AudioRule[]): void {
    void profiles.save(
      profile.id,
      profile.name,
      profile.settings,
      profile.filters,
      rules,
    );
  }

  function addRule(profile: Profile): void {
    updateRules(profile, [...profile.audioRules, defaultAudioRule()]);
  }

  function removeRule(profile: Profile, index: number): void {
    updateRules(
      profile,
      profile.audioRules.filter((_, i) => i !== index),
    );
  }

  function onRuleActionKindChange(
    profile: Profile,
    index: number,
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    const action: AudioAction =
      event.currentTarget.value === "convert"
        ? { action: "convert", codec: "aac", quality: 4, downmix: null }
        : { action: "copy" };
    const rules = profile.audioRules.map((rule, i) =>
      i === index ? { ...rule, action } : rule,
    );
    updateRules(profile, rules);
  }

  function onDownmixChange(
    profile: Profile,
    index: number,
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    const rule = profile.audioRules[index];
    if (rule?.action.action !== "convert") {
      return;
    }
    const raw = event.currentTarget.value;
    const downmix = raw === "" ? null : Number(raw);
    const rules = profile.audioRules.map((r, i) =>
      i === index && r.action.action === "convert"
        ? { ...r, action: { ...r.action, downmix } }
        : r,
    );
    updateRules(profile, rules);
  }

  function onQualityChange(
    profile: Profile,
    index: number,
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    const rule = profile.audioRules[index];
    if (rule?.action.action !== "convert") {
      return;
    }
    const quality = Number(event.currentTarget.value);
    const rules = profile.audioRules.map((r, i) =>
      i === index && r.action.action === "convert"
        ? { ...r, action: { ...r.action, quality } }
        : r,
    );
    updateRules(profile, rules);
  }
</script>

<div class="flex h-full flex-col">
  <header
    class="border-hairline flex h-14 shrink-0 items-center justify-between border-b px-6"
  >
    <h1 class="text-lg font-semibold">Profiles</h1>
    <button
      type="button"
      class="btn btn-primary btn-sm"
      onclick={() => {
        void createFromCurrentSettings();
      }}
    >
      <Plus class="h-4 w-4" />
      New from Current Settings
    </button>
  </header>

  {#if profiles.profiles.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3">
      <Layers class="text-base-content/30 h-8 w-8" />
      <p class="text-base-content/50 text-sm">
        No profiles yet — configure the Convert screen the way you want, then
        save it as a profile.
      </p>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto p-6">
      <div class="flex flex-col gap-3">
        {#each profiles.profiles as profile (profile.id)}
          <div class="bg-base-300 border-hairline rounded-md border">
            <div class="flex h-12 items-center gap-3 px-4">
              <button
                type="button"
                class="btn btn-ghost btn-xs btn-square"
                onclick={() => {
                  expandedId = expandedId === profile.id ? null : profile.id;
                }}
                aria-label="Toggle audio rules"
              >
                {#if expandedId === profile.id}
                  <ChevronDown class="h-3.5 w-3.5" />
                {:else}
                  <ChevronRight class="h-3.5 w-3.5" />
                {/if}
              </button>

              {#if editingId === profile.id}
                <input
                  class="input input-xs"
                  bind:value={editingName}
                  onblur={() => {
                    commitRename(profile);
                  }}
                  onkeydown={(e) => {
                    if (e.key === "Enter") {
                      commitRename(profile);
                    }
                  }}
                />
              {:else}
                <button
                  type="button"
                  class="text-sm font-medium"
                  onclick={() => {
                    startRename(profile);
                  }}
                >
                  {profile.name}
                </button>
              {/if}

              <span class="badge badge-sm badge-outline shrink-0 uppercase">
                {encoderLabel(profile.settings.name)}
              </span>
              {#if profile.audioRules.length > 0}
                <span class="text-base-content/50 text-xs">
                  {profile.audioRules.length}
                  {profile.audioRules.length === 1
                    ? "audio rule"
                    : "audio rules"}
                </span>
              {/if}

              <span class="flex flex-1 items-center justify-end gap-1">
                <button
                  type="button"
                  class="btn btn-ghost btn-xs"
                  onclick={() => {
                    void applyProfile(profile);
                  }}
                >
                  <Play class="h-3.5 w-3.5" />
                  Apply
                </button>
                <button
                  type="button"
                  class="btn btn-ghost btn-xs btn-square"
                  onclick={() => {
                    void profiles.duplicate(profile.id);
                  }}
                  aria-label="Duplicate"
                >
                  <Copy class="h-3.5 w-3.5" />
                </button>
                <button
                  type="button"
                  class="btn btn-ghost btn-xs btn-square"
                  onclick={() => {
                    void profiles.remove(profile.id);
                  }}
                  aria-label="Delete"
                >
                  <Trash2 class="h-3.5 w-3.5" />
                </button>
              </span>
            </div>

            {#if expandedId === profile.id}
              <div class="border-hairline space-y-3 border-t p-4">
                <p class="text-base-content/50 text-xs">
                  Audio rules decide which tracks are enabled (and how they're
                  encoded) when this profile is applied. Rules are evaluated in
                  order — a later rule overrides an earlier one for the same
                  track.
                </p>
                {#each profile.audioRules as rule, i (i)}
                  <div class="flex flex-wrap items-center gap-2">
                    <select
                      class="select select-xs"
                      value={rule.matchItem}
                      onchange={(e) => {
                        updateRules(
                          profile,
                          profile.audioRules.map((r, idx) =>
                            idx === i
                              ? {
                                  ...r,
                                  matchItem: e.currentTarget
                                    .value as AudioRule["matchItem"],
                                }
                              : r,
                          ),
                        );
                      }}
                    >
                      <option value="all">All tracks</option>
                      <option value="language">Language</option>
                      <option value="channels">Channels</option>
                    </select>
                    {#if rule.matchItem !== "all"}
                      <input
                        class="input input-xs w-20"
                        placeholder={rule.matchItem === "language"
                          ? "eng"
                          : "2"}
                        value={rule.matchInput}
                        onchange={(e) => {
                          updateRules(
                            profile,
                            profile.audioRules.map((r, idx) =>
                              idx === i
                                ? { ...r, matchInput: e.currentTarget.value }
                                : r,
                            ),
                          );
                        }}
                      />
                    {/if}
                    <select
                      class="select select-xs"
                      value={rule.matchType}
                      onchange={(e) => {
                        updateRules(
                          profile,
                          profile.audioRules.map((r, idx) =>
                            idx === i
                              ? {
                                  ...r,
                                  matchType: e.currentTarget
                                    .value as AudioRule["matchType"],
                                }
                              : r,
                          ),
                        );
                      }}
                    >
                      <option value="all">Every match</option>
                      <option value="first">First match only</option>
                    </select>

                    <select
                      class="select select-xs"
                      value={rule.action.action}
                      onchange={(e) => {
                        onRuleActionKindChange(profile, i, e);
                      }}
                    >
                      <option value="copy">Copy</option>
                      <option value="convert">Convert</option>
                    </select>
                    {#if rule.action.action === "convert"}
                      {@const action = rule.action}
                      <select
                        class="select select-xs"
                        value={action.codec}
                        onchange={(e) => {
                          updateRules(
                            profile,
                            profile.audioRules.map((r, idx) =>
                              idx === i && r.action.action === "convert"
                                ? {
                                    ...r,
                                    action: {
                                      ...r.action,
                                      codec: e.currentTarget.value,
                                    },
                                  }
                                : r,
                            ),
                          );
                        }}
                      >
                        {#each AUDIO_CODECS as codec (codec.id)}
                          <option value={codec.id}>{codec.label}</option>
                        {/each}
                      </select>
                      {#if !AUDIO_CODECS.find((c) => c.id === action.codec)?.lossless}
                        <input
                          type="range"
                          min="0"
                          max="9"
                          step="1"
                          class="range range-primary range-xs w-16 shrink-0"
                          value={action.quality}
                          oninput={(e) => {
                            onQualityChange(profile, i, e);
                          }}
                          title={`Quality ${String(action.quality)} (0 best, 9 smallest)`}
                        />
                      {/if}
                      <select
                        class="select select-xs"
                        value={action.downmix === null
                          ? ""
                          : String(action.downmix)}
                        onchange={(e) => {
                          onDownmixChange(profile, i, e);
                        }}
                      >
                        <option value="">Source channels</option>
                        <option value="2">Stereo</option>
                        <option value="1">Mono</option>
                      </select>
                    {/if}

                    <button
                      type="button"
                      class="btn btn-ghost btn-xs btn-square"
                      onclick={() => {
                        removeRule(profile, i);
                      }}
                      aria-label="Remove rule"
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </button>
                  </div>
                {/each}
                <button
                  type="button"
                  class="btn btn-ghost btn-xs"
                  onclick={() => {
                    addRule(profile);
                  }}
                >
                  <Plus class="h-3.5 w-3.5" />
                  Add Rule
                </button>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
