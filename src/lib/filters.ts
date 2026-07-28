// Mirrors src-tauri/src/filters.rs's `Crop`/`FilterSpec` wire shape exactly —
// rendering the filter graph stays Rust-only; the frontend only needs the
// data shape to build a form (and, for `crop`, a visual tool) around it.

export interface Crop {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

export interface FilterSpec {
  deinterlace: boolean;
  crop: Crop | null;
  scaleWidth: number | null;
  scaleHeight: number | null;
  rotateDegrees: number;
  horizontalFlip: boolean;
  verticalFlip: boolean;
}

export function defaultFilters(): FilterSpec {
  return {
    deinterlace: false,
    crop: null,
    scaleWidth: null,
    scaleHeight: null,
    rotateDegrees: 0,
    horizontalFlip: false,
    verticalFlip: false,
  };
}
