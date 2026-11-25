// Crop rectangle over the source preview. The rect lives in
// image-relative fractions (0..1), so it survives layout changes and
// maps straight onto natural pixels when the crop is applied.

export interface CropRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

const MIN_SIZE = 0.02;

export function defaultCropRect(): CropRect {
  return { x: 0.05, y: 0.05, w: 0.9, h: 0.9 };
}

export function isFullRect(rect: CropRect): boolean {
  return rect.x <= 0.001 && rect.y <= 0.001 && rect.w >= 0.999 && rect.h >= 0.999;
}

type DragMode = 'move' | 'draw' | 'nw' | 'ne' | 'sw' | 'se';

export class CropController {
  private rect: CropRect = defaultCropRect();
  private drag: {
    mode: DragMode;
    startX: number;
    startY: number;
    orig: CropRect;
  } | null = null;

  constructor(
    private overlay: HTMLElement,
    private rectEl: HTMLElement,
  ) {
    overlay.addEventListener('pointerdown', this.onPointerDown);
    overlay.addEventListener('pointermove', this.onPointerMove);
    overlay.addEventListener('pointerup', this.onPointerUp);
    overlay.addEventListener('pointercancel', this.onPointerUp);
  }

  getRect(): CropRect {
    return { ...this.rect };
  }

  setRect(rect: CropRect): void {
    this.rect = { ...rect };
    this.render();
  }

  show(): void {
    this.overlay.hidden = false;
    this.render();
  }

  hide(): void {
    this.overlay.hidden = true;
    this.drag = null;
  }

  private render(): void {
    const pct = (v: number) => `${v * 100}%`;
    this.rectEl.style.left = pct(this.rect.x);
    this.rectEl.style.top = pct(this.rect.y);
    this.rectEl.style.width = pct(this.rect.w);
    this.rectEl.style.height = pct(this.rect.h);
  }

  private toFraction(event: PointerEvent): { fx: number; fy: number } {
    const box = this.overlay.getBoundingClientRect();
    const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
    return {
      fx: clamp01((event.clientX - box.left) / box.width),
      fy: clamp01((event.clientY - box.top) / box.height),
    };
  }

  private onPointerDown = (event: PointerEvent): void => {
    event.preventDefault();
    const target = event.target as HTMLElement;
    const handle = target.dataset.handle as DragMode | undefined;
    let mode: DragMode;
    if (handle) {
      mode = handle;
    } else if (target === this.rectEl) {
      mode = 'move';
    } else {
      mode = 'draw';
    }
    const { fx, fy } = this.toFraction(event);
    if (mode === 'draw') {
      this.rect = { x: fx, y: fy, w: 0, h: 0 };
    }
    this.drag = { mode, startX: fx, startY: fy, orig: { ...this.rect } };
    this.overlay.setPointerCapture(event.pointerId);
    this.render();
  };

  private onPointerMove = (event: PointerEvent): void => {
    if (!this.drag) return;
    const { fx, fy } = this.toFraction(event);
    const { mode, startX, startY, orig } = this.drag;
    const dx = fx - startX;
    const dy = fy - startY;
    const clamp = (v: number, lo: number, hi: number) =>
      Math.min(hi, Math.max(lo, v));
    switch (mode) {
      case 'move':
        this.rect = {
          ...orig,
          x: clamp(orig.x + dx, 0, 1 - orig.w),
          y: clamp(orig.y + dy, 0, 1 - orig.h),
        };
        break;
      case 'draw': {
        const x = Math.min(startX, fx);
        const y = Math.min(startY, fy);
        this.rect = { x, y, w: Math.abs(fx - startX), h: Math.abs(fy - startY) };
        break;
      }
      case 'nw':
      case 'ne':
      case 'sw':
      case 'se': {
        let { x, y, w, h } = orig;
        if (mode === 'nw' || mode === 'sw') {
          const right = orig.x + orig.w;
          x = clamp(fx, 0, right - MIN_SIZE);
          w = right - x;
        }
        if (mode === 'ne' || mode === 'se') {
          w = clamp(fx - orig.x, MIN_SIZE, 1 - orig.x);
        }
        if (mode === 'nw' || mode === 'ne') {
          const bottom = orig.y + orig.h;
          y = clamp(fy, 0, bottom - MIN_SIZE);
          h = bottom - y;
        }
        if (mode === 'sw' || mode === 'se') {
          h = clamp(fy - orig.y, MIN_SIZE, 1 - orig.y);
        }
        this.rect = { x, y, w, h };
        break;
      }
    }
    this.render();
  };

  private onPointerUp = (event: PointerEvent): void => {
    if (!this.drag) return;
    // A click-draw with no real drag leaves a sliver; snap it back to
    // the default rect instead of keeping an unusable selection.
    if (this.drag.mode === 'draw' && (this.rect.w < MIN_SIZE || this.rect.h < MIN_SIZE)) {
      this.rect = this.drag.orig.w > 0 ? this.drag.orig : defaultCropRect();
    }
    if (this.rect.w < MIN_SIZE) this.rect.w = MIN_SIZE;
    if (this.rect.h < MIN_SIZE) this.rect.h = MIN_SIZE;
    this.drag = null;
    try {
      this.overlay.releasePointerCapture(event.pointerId);
    } catch {
      // Capture may already be released; nothing to do.
    }
    this.render();
  };
}
