<script lang="ts">
  import { onMount } from "svelte";
  import RefreshCw from "lucide-svelte/icons/refresh-cw";
  import Clock from "lucide-svelte/icons/clock";
  import CheckCircle2 from "lucide-svelte/icons/check-circle-2";
  import AlertCircle from "lucide-svelte/icons/alert-circle";
  import Pause from "lucide-svelte/icons/pause";
  import Play from "lucide-svelte/icons/play";
  import ListOrdered from "lucide-svelte/icons/list-ordered";
  import { queue, type Job, type JobStatus } from "$lib/stores/queue.svelte";
  import QueueJobRow from "$lib/components/QueueJobRow.svelte";

  onMount(() => {
    void queue.refresh();
  });

  const sections = $derived([
    {
      kind: "running",
      title: "Encoding",
      icon: RefreshCw,
      iconClass: "text-warning",
      jobs: queue.jobs.filter((j) => j.status.state === "running"),
    },
    {
      kind: "queued",
      title: "Queued",
      icon: Clock,
      iconClass: "text-info",
      jobs: queue.jobs.filter((j) => j.status.state === "queued"),
    },
    {
      kind: "completed",
      title: "Completed",
      icon: CheckCircle2,
      iconClass: "text-success",
      jobs: queue.jobs.filter((j) => j.status.state === "completed"),
    },
    {
      kind: "failed",
      title: "Failed",
      icon: AlertCircle,
      iconClass: "text-error",
      jobs: queue.jobs.filter(
        (j) => j.status.state === "failed" || j.status.state === "cancelled",
      ),
    },
  ]);

  const completedJobs = $derived(
    queue.jobs.filter((j) => j.status.state === "completed"),
  );

  function segmentClass(status: JobStatus): string {
    switch (status.state) {
      case "completed":
        return "bg-primary";
      case "running":
        return "bg-warning animate-pulse";
      case "failed":
        return "bg-error/40";
      case "cancelled":
        return "bg-base-content/20";
      case "queued":
        return "border-hairline border";
    }
  }

  async function clearCompleted(): Promise<void> {
    for (const job of completedJobs) {
      await queue.remove(job.id);
    }
  }

  async function togglePause(): Promise<void> {
    if (queue.paused) {
      await queue.resume();
    } else {
      await queue.pause();
    }
  }

  function jobKey(job: Job): string {
    return job.id;
  }
</script>

<div class="flex h-full flex-col">
  <header
    class="border-hairline flex h-14 shrink-0 items-center justify-between border-b px-6"
  >
    <div class="flex items-center gap-2">
      <h1 class="text-lg font-semibold">Queue</h1>
      <span class="badge badge-sm badge-outline">
        {queue.jobs.length}
        {queue.jobs.length === 1 ? "job" : "jobs"}
      </span>
    </div>
    <div class="flex items-center gap-2">
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        disabled={completedJobs.length === 0}
        onclick={() => {
          void clearCompleted();
        }}
      >
        Clear Completed
      </button>
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        onclick={() => {
          void togglePause();
        }}
      >
        {#if queue.paused}
          <Play class="h-4 w-4" />
          Resume Queue
        {:else}
          <Pause class="h-4 w-4" />
          Pause Queue
        {/if}
      </button>
    </div>
  </header>

  {#if queue.jobs.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3">
      <ListOrdered class="text-base-content/30 h-8 w-8" />
      <p class="text-base-content/50 text-sm">
        Nothing queued yet — add a job from the Convert screen.
      </p>
    </div>
  {:else}
    <div class="px-6 pt-6">
      <div class="bg-base-300 border-hairline rounded-lg border p-4">
        <div class="mb-2 flex items-center justify-between">
          <span class="text-xs font-semibold">
            {completedJobs.length} of {queue.jobs.length} jobs complete
          </span>
          {#if queue.paused}
            <span class="text-warning text-xs font-medium">Paused</span>
          {/if}
        </div>
        <div class="flex h-1.5 gap-1">
          {#each queue.jobs as job (job.id)}
            <div class="flex-1 rounded-full {segmentClass(job.status)}"></div>
          {/each}
        </div>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto p-6">
      {#each sections as sectionData (sectionData.kind)}
        {#if sectionData.jobs.length > 0}
          {@const SectionIcon = sectionData.icon}
          <section class="mb-6 flex flex-col">
            <div class="mb-1 flex items-center gap-2 px-1 py-2">
              <SectionIcon class="h-3.5 w-3.5 {sectionData.iconClass}" />
              <span
                class="text-base-content/80 text-[11px] font-semibold tracking-wider uppercase"
              >
                {sectionData.title}
              </span>
              <span class="text-base-content/50 text-[11px]">
                {sectionData.jobs.length}
              </span>
            </div>
            <div
              class="bg-base-300 border-hairline divide-hairline divide-y rounded-md border"
            >
              {#each sectionData.jobs as job, i (jobKey(job))}
                <QueueJobRow {job} index={i} total={sectionData.jobs.length} />
              {/each}
            </div>
          </section>
        {/if}
      {/each}
    </div>
  {/if}
</div>
