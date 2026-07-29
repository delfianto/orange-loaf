<script lang="ts">
  import { onMount } from "svelte";
  import IconRail from "$lib/components/IconRail.svelte";
  import { navigation } from "$lib/stores/navigation.svelte";
  import { config } from "$lib/stores/config.svelte";
  import ConvertView from "$lib/views/ConvertView.svelte";
  import QueueView from "$lib/views/QueueView.svelte";
  import HistoryView from "$lib/views/HistoryView.svelte";
  import ProfilesView from "$lib/views/ProfilesView.svelte";
  import SettingsView from "$lib/views/SettingsView.svelte";

  onMount(() => {
    void config.refresh();
  });

  // Applied at the document root (not scoped to one view) so a density
  // change is visible everywhere at once, matching DaisyUI's own
  // `data-theme` convention — see app.css's `[data-density="compact"]`
  // rules for what this actually changes.
  $effect(() => {
    document.documentElement.dataset.density =
      config.data?.uiDensity ?? "comfortable";
  });
</script>

<div
  class="bg-base-100 text-base-content flex h-screen w-screen overflow-hidden"
>
  <IconRail />
  <main class="flex-1 overflow-hidden">
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
  </main>
</div>
