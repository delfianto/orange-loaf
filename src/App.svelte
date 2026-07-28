<script lang="ts">
  import IconRail from "$lib/components/IconRail.svelte";
  import { navigation } from "$lib/stores/navigation.svelte";
  import ConvertView from "$lib/views/ConvertView.svelte";
  import QueueView from "$lib/views/QueueView.svelte";
  import HistoryView from "$lib/views/HistoryView.svelte";
  import ProfilesView from "$lib/views/ProfilesView.svelte";
  import SettingsView from "$lib/views/SettingsView.svelte";
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
