<script lang="ts">
  import { onMount } from "svelte";
  import IconRail from "$lib/components/IconRail.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import { navigation } from "$lib/stores/navigation.svelte";
  import { config } from "$lib/stores/config.svelte";
  import { tools } from "$lib/stores/tools.svelte";
  import { queue } from "$lib/stores/queue.svelte";
  import { i18n, isLocale } from "$lib/i18n.svelte";
  import ConvertView from "$lib/views/ConvertView.svelte";
  import QueueView from "$lib/views/QueueView.svelte";
  import HistoryView from "$lib/views/HistoryView.svelte";
  import ProfilesView from "$lib/views/ProfilesView.svelte";
  import SettingsView from "$lib/views/SettingsView.svelte";

  onMount(() => {
    void config.refresh();
    // The status bar is mounted here, persistently across every view, so
    // it needs both stores initialized regardless of which view (if any)
    // would otherwise have triggered them first.
    void tools.refresh();
    void queue.refresh();
  });

  // Applied at the document root (not scoped to one view) so a density
  // change is visible everywhere at once, matching DaisyUI's own
  // `data-theme` convention — see app.css's `[data-density="compact"]`
  // rules for what this actually changes.
  $effect(() => {
    document.documentElement.dataset.density =
      config.data?.uiDensity ?? "comfortable";
  });

  // Applies whatever language was already persisted (M12) the first time
  // it becomes available, so a relaunch actually reopens in that language
  // instead of always starting from paraglide's English default.
  $effect(() => {
    const language = config.data?.language;
    if (language && isLocale(language) && language !== i18n.locale) {
      i18n.setLocale(language);
    }
  });
</script>

<div
  class="bg-base-100 text-base-content flex h-screen w-screen overflow-hidden"
>
  <!-- Remounts on locale change so every `m.xxx()` call underneath
       re-evaluates against the new locale — see i18n.svelte.ts's own doc
       comment for why this is necessary at all. -->
  {#key i18n.locale}
    <IconRail />
    <main class="flex flex-1 flex-col overflow-hidden">
      <div class="flex-1 overflow-hidden">
        {#if navigation.current === "convert"}
          <ConvertView />
        {:else if navigation.current === "queue"}
          <QueueView />
        {:else if navigation.current === "history"}
          <HistoryView />
        {:else if navigation.current === "profiles"}
          <ProfilesView />
        {:else}
          <SettingsView />
        {/if}
      </div>
      <StatusBar />
    </main>
  {/key}
</div>
