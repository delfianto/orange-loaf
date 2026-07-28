<script lang="ts">
  import type { Crop } from "$lib/filters";
  import {
    containRect,
    cropToRect,
    moveRect,
    pointerToNormalized,
    rectToCrop,
    resizeRect,
    sanitizeCrop,
    type CropHandle,
    type NormalizedPoint,
    type NormalizedRect,
  } from "$lib/crop";

  interface Props {
    sourceWidth: number;
    sourceHeight: number;
    crop: Crop;
    onChange: (crop: Crop) => void;
  }

  const { sourceWidth, sourceHeight, crop, onChange }: Props = $props();

  let rootEl: HTMLDivElement | undefined = $state();
  let containerWidth = $state(0);
  let containerHeight = $state(0);

  const imageRect = $derived(
    containRect(containerWidth, containerHeight, sourceWidth, sourceHeight),
  );
  const rect = $derived(cropToRect(crop, sourceWidth, sourceHeight));

  interface DragState {
    handle: CropHandle;
    startRect: NormalizedRect;
    startPointer: NormalizedPoint;
  }
  let drag: DragState | null = $state(null);

  function toNormalized(event: PointerEvent): NormalizedPoint {
    if (!rootEl) {
      return { x: 0, y: 0 };
    }
    const containerRect = rootEl.getBoundingClientRect();
    return pointerToNormalized(
      event.clientX - containerRect.left,
      event.clientY - containerRect.top,
      imageRect,
    );
  }

  function beginDrag(handle: CropHandle, event: PointerEvent): void {
    event.preventDefault();
    event.stopPropagation();
    if (event.currentTarget instanceof HTMLElement) {
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    drag = { handle, startRect: rect, startPointer: toNormalized(event) };
  }

  function onDragMove(event: PointerEvent): void {
    if (!drag) {
      return;
    }
    const point = toNormalized(event);
    const next =
      drag.handle === "move"
        ? moveRect(
            drag.startRect,
            point.x - drag.startPointer.x,
            point.y - drag.startPointer.y,
          )
        : resizeRect(drag.startRect, drag.handle, point);
    onChange(
      sanitizeCrop(
        rectToCrop(next, sourceWidth, sourceHeight),
        sourceWidth,
        sourceHeight,
      ),
    );
  }

  function endDrag(event: PointerEvent): void {
    if (!drag) {
      return;
    }
    if (event.currentTarget instanceof HTMLElement) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    drag = null;
  }

  const RESIZE_HANDLES: Exclude<CropHandle, "move">[] = [
    "nw",
    "n",
    "ne",
    "e",
    "se",
    "s",
    "sw",
    "w",
  ];

  const HANDLE_POSITION: Record<
    Exclude<CropHandle, "move">,
    { left: string; top: string }
  > = {
    n: { left: "50%", top: "0%" },
    s: { left: "50%", top: "100%" },
    e: { left: "100%", top: "50%" },
    w: { left: "0%", top: "50%" },
    ne: { left: "100%", top: "0%" },
    nw: { left: "0%", top: "0%" },
    se: { left: "100%", top: "100%" },
    sw: { left: "0%", top: "100%" },
  };

  const CURSOR: Record<CropHandle, string> = {
    move: "move",
    n: "ns-resize",
    s: "ns-resize",
    e: "ew-resize",
    w: "ew-resize",
    ne: "nesw-resize",
    sw: "nesw-resize",
    nw: "nwse-resize",
    se: "nwse-resize",
  };
</script>

<div
  bind:this={rootEl}
  bind:clientWidth={containerWidth}
  bind:clientHeight={containerHeight}
  class="absolute inset-0"
>
  {#if imageRect.width > 0}
    <!-- Clips the crop rect's box-shadow spread (below) to exactly the
         rendered frame, so the "outside the selection" dimming can't bleed
         past the video's own edges into the rest of the preview panel. -->
    <div
      class="absolute overflow-hidden"
      style:left="{imageRect.left}px"
      style:top="{imageRect.top}px"
      style:width="{imageRect.width}px"
      style:height="{imageRect.height}px"
    >
      <div
        class="border-primary absolute border-2"
        style:left="{rect.x * imageRect.width}px"
        style:top="{rect.y * imageRect.height}px"
        style:width="{rect.width * imageRect.width}px"
        style:height="{rect.height * imageRect.height}px"
        style:cursor={CURSOR.move}
        style:box-shadow="0 0 0 9999px rgba(0, 0, 0, 0.55)"
        onpointerdown={(event) => {
          beginDrag("move", event);
        }}
        onpointermove={onDragMove}
        onpointerup={endDrag}
        onpointercancel={endDrag}
        role="presentation"
      >
        {#each RESIZE_HANDLES as handle (handle)}
          <div
            class="border-primary bg-base-100 absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border"
            style:left={HANDLE_POSITION[handle].left}
            style:top={HANDLE_POSITION[handle].top}
            style:cursor={CURSOR[handle]}
            onpointerdown={(event) => {
              beginDrag(handle, event);
            }}
            onpointermove={onDragMove}
            onpointerup={endDrag}
            onpointercancel={endDrag}
            role="presentation"
          ></div>
        {/each}
      </div>
    </div>
  {/if}
</div>
