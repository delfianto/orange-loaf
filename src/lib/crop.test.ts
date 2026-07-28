import { describe, expect, it } from "vitest";
import {
  containRect,
  cropToRect,
  moveRect,
  pointerToNormalized,
  rectToCrop,
  resizeRect,
  sanitizeCrop,
  snapCropToEven,
  type NormalizedRect,
} from "$lib/crop";

// Several ops below subtract/add decimal literals (0.7 - 0.2, 1 - 0.4, ...),
// which binary floating point does not represent exactly — asserting via
// `toEqual` on the whole object would make these tests flaky/wrong for
// reasons that have nothing to do with the logic under test.
function expectRectCloseTo(
  actual: NormalizedRect,
  expected: NormalizedRect,
): void {
  expect(actual.x).toBeCloseTo(expected.x, 5);
  expect(actual.y).toBeCloseTo(expected.y, 5);
  expect(actual.width).toBeCloseTo(expected.width, 5);
  expect(actual.height).toBeCloseTo(expected.height, 5);
}

describe("containRect", () => {
  it("letterboxes top and bottom when content is wider than the container", () => {
    // 16:9 content in a square container: full width, centered height band.
    const rect = containRect(200, 200, 1920, 1080);
    expect(rect.left).toBe(0);
    expect(rect.width).toBeCloseTo(200, 5);
    expect(rect.height).toBeCloseTo(112.5, 5);
    expect(rect.top).toBeCloseTo(43.75, 5);
  });

  it("pillarboxes left and right when content is taller than the container", () => {
    // 9:16 (vertical) content in a 16:9 container.
    const rect = containRect(1600, 900, 1080, 1920);
    expect(rect.top).toBe(0);
    expect(rect.height).toBeCloseTo(900, 5);
    expect(rect.left).toBeCloseTo(546.875, 5);
  });

  it("fills the container exactly when aspect ratios match", () => {
    const rect = containRect(1600, 900, 1920, 1080);
    expect(rect.left).toBeCloseTo(0, 5);
    expect(rect.top).toBeCloseTo(0, 5);
    expect(rect.width).toBeCloseTo(1600, 5);
    expect(rect.height).toBeCloseTo(900, 5);
  });

  it("returns a zero rect instead of dividing by zero when a dimension is missing", () => {
    expect(containRect(0, 0, 1920, 1080)).toEqual({
      left: 0,
      top: 0,
      width: 0,
      height: 0,
    });
  });
});

describe("pointerToNormalized", () => {
  it("maps a point at the image's top-left corner to (0, 0)", () => {
    const imageRect = { left: 10, top: 20, width: 100, height: 50 };
    expect(pointerToNormalized(10, 20, imageRect)).toEqual({ x: 0, y: 0 });
  });

  it("maps a point at the image's center to (0.5, 0.5)", () => {
    const imageRect = { left: 10, top: 20, width: 100, height: 50 };
    const point = pointerToNormalized(60, 45, imageRect);
    expect(point.x).toBeCloseTo(0.5, 5);
    expect(point.y).toBeCloseTo(0.5, 5);
  });

  it("does not divide by zero for a degenerate (unmeasured) image rect", () => {
    const imageRect = { left: 0, top: 0, width: 0, height: 0 };
    expect(pointerToNormalized(5, 5, imageRect)).toEqual({ x: 0, y: 0 });
  });
});

describe("cropToRect / rectToCrop round-trip", () => {
  it("round-trips a crop through normalized-rect space unchanged", () => {
    const crop = { top: 100, bottom: 40, left: 60, right: 20 };
    const rect = cropToRect(crop, 1920, 1080);
    expect(rectToCrop(rect, 1920, 1080)).toEqual(crop);
  });

  it("treats a zeroed crop as the full frame", () => {
    const rect = cropToRect(
      { top: 0, bottom: 0, left: 0, right: 0 },
      1920,
      1080,
    );
    expect(rect).toEqual({ x: 0, y: 0, width: 1, height: 1 });
  });
});

describe("moveRect", () => {
  const rect = { x: 0.2, y: 0.2, width: 0.4, height: 0.3 };

  it("translates without changing size", () => {
    expectRectCloseTo(moveRect(rect, 0.1, -0.05), {
      x: 0.3,
      y: 0.15,
      width: 0.4,
      height: 0.3,
    });
  });

  it("clamps so the rect cannot be dragged past the frame's right/bottom edge", () => {
    const moved = moveRect(rect, 10, 10);
    expect(moved.x).toBeCloseTo(0.6, 5);
    expect(moved.y).toBeCloseTo(0.7, 5);
  });

  it("clamps so the rect cannot be dragged past the frame's left/top edge", () => {
    const moved = moveRect(rect, -10, -10);
    expect(moved.x).toBe(0);
    expect(moved.y).toBe(0);
  });
});

describe("resizeRect", () => {
  const rect = { x: 0.2, y: 0.2, width: 0.4, height: 0.4 };

  it("dragging the east handle moves only the right edge", () => {
    expectRectCloseTo(resizeRect(rect, "e", { x: 0.7, y: 0.9 }), {
      x: 0.2,
      y: 0.2,
      width: 0.5,
      height: 0.4,
    });
  });

  it("dragging the west handle moves only the left edge", () => {
    expectRectCloseTo(resizeRect(rect, "w", { x: 0.1, y: 0.9 }), {
      x: 0.1,
      y: 0.2,
      width: 0.5,
      height: 0.4,
    });
  });

  it("dragging a corner handle moves both of its edges", () => {
    expectRectCloseTo(resizeRect(rect, "se", { x: 0.8, y: 0.5 }), {
      x: 0.2,
      y: 0.2,
      width: 0.6,
      height: 0.3,
    });
  });

  it("stops at MIN_DIMENSION instead of inverting when dragged past the opposite edge", () => {
    const resized = resizeRect(rect, "e", { x: 0, y: 0.9 });
    expect(resized.width).toBeCloseTo(0.02, 5);
  });

  it("clamps the pointer into [0, 1] instead of producing an out-of-frame rect", () => {
    const resized = resizeRect(rect, "e", { x: 5, y: 0.9 });
    expect(resized.width).toBeCloseTo(0.8, 5);
  });
});

describe("snapCropToEven", () => {
  it("leaves an already-even crop untouched", () => {
    const crop = { top: 10, bottom: 10, left: 20, right: 20 };
    expect(snapCropToEven(crop, 1920, 1080)).toEqual(crop);
  });

  it("produces an even output width and height, and even left/top offsets, from odd input", () => {
    const crop = { top: 1, bottom: 1, left: 101, right: 100 };
    const snapped = snapCropToEven(crop, 1920, 1080);
    expect((1920 - snapped.left - snapped.right) % 2).toBe(0);
    expect((1080 - snapped.top - snapped.bottom) % 2).toBe(0);
    expect(snapped.left % 2).toBe(0);
    expect(snapped.top % 2).toBe(0);
  });
});

describe("sanitizeCrop", () => {
  it("leaves a valid, already-even crop unchanged", () => {
    const crop = { top: 10, bottom: 10, left: 20, right: 20 };
    expect(sanitizeCrop(crop, 1920, 1080)).toEqual(crop);
  });

  it("brings an out-of-bounds hand-typed crop back to a non-degenerate, in-frame rect", () => {
    // left + right (5000) far exceeds the 1920-wide source.
    const crop = { top: 0, bottom: 0, left: 5000, right: 0 };
    const sanitized = sanitizeCrop(crop, 1920, 1080);
    const width = 1920 - sanitized.left - sanitized.right;
    const height = 1080 - sanitized.top - sanitized.bottom;
    expect(width).toBeGreaterThan(0);
    expect(height).toBeGreaterThan(0);
    expect(width % 2).toBe(0);
    expect(sanitized.left).toBeLessThan(1920);
  });

  it("rejects a negative crop by clamping it back to non-negative", () => {
    const crop = { top: -50, bottom: 0, left: -50, right: 0 };
    const sanitized = sanitizeCrop(crop, 1920, 1080);
    expect(sanitized.top).toBeGreaterThanOrEqual(0);
    expect(sanitized.left).toBeGreaterThanOrEqual(0);
  });
});
