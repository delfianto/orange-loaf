import type { Crop } from "$lib/filters";

/**
 * A selection expressed as fractions (0..1) of the source frame, rather
 * than pixels — the natural space for drag math, independent of both the
 * source's resolution and however large the preview happens to render.
 */
export interface NormalizedRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface NormalizedPoint {
  x: number;
  y: number;
}

// Where the source frame actually lands inside its (possibly
// differently-shaped) preview container — i.e. `object-contain`'s letterbox
// math, computed by hand because there's no CSS query for "the rendered
// content box of an object-contain image."
export interface PixelRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export type CropHandle =
  "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

// Below this, a handle drag couldn't produce a sane crop (zero/negative
// width or height) and the rectangle stops shrinking instead of inverting.
const MIN_DIMENSION = 0.02;

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function containRect(
  containerWidth: number,
  containerHeight: number,
  contentWidth: number,
  contentHeight: number,
): PixelRect {
  if (
    containerWidth <= 0 ||
    containerHeight <= 0 ||
    contentWidth <= 0 ||
    contentHeight <= 0
  ) {
    return { left: 0, top: 0, width: 0, height: 0 };
  }
  const containerAspect = containerWidth / containerHeight;
  const contentAspect = contentWidth / contentHeight;
  if (contentAspect > containerAspect) {
    const width = containerWidth;
    const height = width / contentAspect;
    return { left: 0, top: (containerHeight - height) / 2, width, height };
  }
  const height = containerHeight;
  const width = height * contentAspect;
  return { left: (containerWidth - width) / 2, top: 0, width, height };
}

export function pointerToNormalized(
  containerRelativeX: number,
  containerRelativeY: number,
  imageRect: PixelRect,
): NormalizedPoint {
  if (imageRect.width === 0 || imageRect.height === 0) {
    return { x: 0, y: 0 };
  }
  return {
    x: (containerRelativeX - imageRect.left) / imageRect.width,
    y: (containerRelativeY - imageRect.top) / imageRect.height,
  };
}

export function cropToRect(
  crop: Crop,
  sourceWidth: number,
  sourceHeight: number,
): NormalizedRect {
  const x = crop.left / sourceWidth;
  const y = crop.top / sourceHeight;
  return {
    x,
    y,
    width: 1 - x - crop.right / sourceWidth,
    height: 1 - y - crop.bottom / sourceHeight,
  };
}

export function rectToCrop(
  rect: NormalizedRect,
  sourceWidth: number,
  sourceHeight: number,
): Crop {
  const left = rect.x * sourceWidth;
  const top = rect.y * sourceHeight;
  return {
    left: Math.round(left),
    top: Math.round(top),
    right: Math.round(sourceWidth - left - rect.width * sourceWidth),
    bottom: Math.round(sourceHeight - top - rect.height * sourceHeight),
  };
}

function roundToEven(value: number): number {
  return 2 * Math.round(value / 2);
}

/**
 * ffmpeg errors when a 4:2:0 encode's final width/height is odd, and even
 * when it doesn't, an odd crop offset shifts chroma siting by half a pixel.
 * Snaps both away — safe to call on any in-bounds crop (`left + right <
 * sourceWidth`, same for top/bottom/height), which is what `clampRect`
 * guarantees.
 */
export function snapCropToEven(
  crop: Crop,
  sourceWidth: number,
  sourceHeight: number,
): Crop {
  const left = roundToEven(crop.left);
  const top = roundToEven(crop.top);
  const width = roundToEven(sourceWidth - left - crop.right);
  const height = roundToEven(sourceHeight - top - crop.bottom);
  return {
    left,
    top,
    right: Math.max(sourceWidth - left - width, 0),
    bottom: Math.max(sourceHeight - top - height, 0),
  };
}

function clampRect(rect: NormalizedRect): NormalizedRect {
  const width = clamp(rect.width, MIN_DIMENSION, 1);
  const height = clamp(rect.height, MIN_DIMENSION, 1);
  return {
    x: clamp(rect.x, 0, 1 - width),
    y: clamp(rect.y, 0, 1 - height),
    width,
    height,
  };
}

/**
 * Brings an arbitrary (possibly out-of-bounds or hand-typed) crop back into
 * a crop `ffmpeg` can actually run: in-frame, non-degenerate, even offsets
 * and dimensions. The drag tool's own moves are already in-bounds by
 * construction (see `moveRect`/`resizeRect`), but numeric-field edits have
 * no such guarantee, so both paths route through this single function
 * rather than duplicating "what counts as a valid crop."
 */
export function sanitizeCrop(
  crop: Crop,
  sourceWidth: number,
  sourceHeight: number,
): Crop {
  const rect = clampRect(cropToRect(crop, sourceWidth, sourceHeight));
  return snapCropToEven(
    rectToCrop(rect, sourceWidth, sourceHeight),
    sourceWidth,
    sourceHeight,
  );
}

export function moveRect(
  startRect: NormalizedRect,
  dx: number,
  dy: number,
): NormalizedRect {
  const { width, height } = startRect;
  return {
    x: clamp(startRect.x + dx, 0, 1 - width),
    y: clamp(startRect.y + dy, 0, 1 - height),
    width,
    height,
  };
}

const WEST_HANDLES = new Set<CropHandle>(["w", "nw", "sw"]);
const EAST_HANDLES = new Set<CropHandle>(["e", "ne", "se"]);
const NORTH_HANDLES = new Set<CropHandle>(["n", "nw", "ne"]);
const SOUTH_HANDLES = new Set<CropHandle>(["s", "sw", "se"]);

/**
 * Resizes by moving only the edges the given handle owns, anchored on the
 * opposite edge — e.g. dragging "w" moves the left edge and holds the
 * right edge fixed at its drag-start position. This anchor-based approach
 * (versus tracking a delta) is what makes inversion impossible: the moving
 * edge is always clamped against the fixed one before the new width/height
 * is derived from them, so it can shrink to `MIN_DIMENSION` and stop.
 */
export function resizeRect(
  startRect: NormalizedRect,
  handle: Exclude<CropHandle, "move">,
  pointer: NormalizedPoint,
): NormalizedRect {
  const px = clamp(pointer.x, 0, 1);
  const py = clamp(pointer.y, 0, 1);

  let left = startRect.x;
  let right = startRect.x + startRect.width;
  let top = startRect.y;
  let bottom = startRect.y + startRect.height;

  if (WEST_HANDLES.has(handle)) {
    left = Math.min(px, right - MIN_DIMENSION);
  }
  if (EAST_HANDLES.has(handle)) {
    right = Math.max(px, left + MIN_DIMENSION);
  }
  if (NORTH_HANDLES.has(handle)) {
    top = Math.min(py, bottom - MIN_DIMENSION);
  }
  if (SOUTH_HANDLES.has(handle)) {
    bottom = Math.max(py, top + MIN_DIMENSION);
  }

  return { x: left, y: top, width: right - left, height: bottom - top };
}
