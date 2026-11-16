// Input normalization: every input route ends as a Blob handed to main —
// file picker, drag & drop, paste (event and clipboard-read button), and
// the Web Share Target intake stashed by the service worker.

export interface FileInputHooks {
  input: HTMLInputElement;
  zone: HTMLElement;
  button: HTMLElement;
  onBlob: (blob: Blob) => void;
  onInvalid: () => void;
}

export function wireFilePicker(hooks: FileInputHooks): void {
  const open = () => hooks.input.click();
  hooks.button.addEventListener('click', open);
  hooks.zone.addEventListener('click', open);
  hooks.zone.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      open();
    }
  });
  hooks.input.addEventListener('change', () => {
    const file = hooks.input.files?.[0];
    hooks.input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      hooks.onInvalid();
      return;
    }
    hooks.onBlob(file);
  });
}

function firstImageFile(list: FileList | null | undefined): File | null {
  if (!list) return null;
  for (const file of Array.from(list)) {
    if (file.type.startsWith('image/')) return file;
  }
  return null;
}

export interface DropHooks {
  zone: HTMLElement;
  onBlob: (blob: Blob) => void;
  onInvalid: () => void;
}

/// Page-wide drag & drop: dragging anywhere highlights the drop zone;
/// dropping hands the first image file to the app. The browser's default
/// "open the dropped file" behaviour is always suppressed.
export function wireDragAndDrop(hooks: DropHooks): void {
  let depth = 0;
  const hasFiles = (event: DragEvent): boolean =>
    Array.from(event.dataTransfer?.types ?? []).includes('Files');
  window.addEventListener('dragenter', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    depth += 1;
    hooks.zone.classList.add('drag-over');
  });
  window.addEventListener('dragover', (event) => {
    if (hasFiles(event)) event.preventDefault();
  });
  window.addEventListener('dragleave', (event) => {
    if (!hasFiles(event)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) hooks.zone.classList.remove('drag-over');
  });
  window.addEventListener('drop', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    depth = 0;
    hooks.zone.classList.remove('drag-over');
    const transfer = event.dataTransfer;
    if (!transfer || transfer.files.length === 0) return;
    const image = firstImageFile(transfer.files);
    if (image) hooks.onBlob(image);
    else hooks.onInvalid();
  });
}

/// Ctrl/Cmd+V paste. Pastes aimed at a form field (e.g. the custom width
/// input) keep their normal text behaviour and are not hijacked.
export function wirePaste(hooks: { onBlob: (blob: Blob) => void }): void {
  document.addEventListener('paste', (event) => {
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))
    ) {
      return;
    }
    const data = event.clipboardData;
    if (!data) return;
    const image =
      firstImageFile(data.files) ??
      Array.from(data.items)
        .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
        .map((item) => item.getAsFile())
        .find((file) => file !== null) ??
      null;
    if (image) {
      event.preventDefault();
      hooks.onBlob(image);
    }
  });
}

export function canReadClipboard(): boolean {
  return (
    typeof navigator !== 'undefined' &&
    !!navigator.clipboard &&
    typeof navigator.clipboard.read === 'function'
  );
}

export type ClipboardImageResult =
  | { blob: Blob }
  | { error: 'empty' | 'failed' };

/// The "paste" button route: reads the clipboard on the user's tap.
/// 'empty' = no image on the clipboard; 'failed' = read refused or
/// unsupported (the caller suggests the file picker instead).
export async function readClipboardImage(): Promise<ClipboardImageResult> {
  if (!canReadClipboard()) return { error: 'failed' };
  let items: ClipboardItem[];
  try {
    items = await navigator.clipboard.read();
  } catch {
    return { error: 'failed' };
  }
  for (const item of items) {
    const type = item.types.find((t) => t.startsWith('image/'));
    if (!type) continue;
    try {
      return { blob: await item.getType(type) };
    } catch {
      return { error: 'failed' };
    }
  }
  return { error: 'empty' };
}

// --- Share-target intake -------------------------------------------------
// Mirrors sw.js: the worker stashes shared images in this cache under
// `shared-image/<id>` with an `x-pic2gray4px-saved-at` header, and lands
// the user on `./?share=<id>`. The app consumes each image exactly once.
// Leftovers are swept by age (TTL), never wholesale: another tab may be
// about to consume its own share.

const SHARE_CACHE = 'pic2gray4px-share';
const SHARE_TTL_MS = 10 * 60 * 1000;
const SAVED_AT_HEADER = 'x-pic2gray4px-saved-at';

function shareKey(id: string): string {
  return new URL(`shared-image/${id}`, new URL('./', document.baseURI)).href;
}

function isStale(response: Response): boolean {
  const savedAt = Number(response.headers.get(SAVED_AT_HEADER));
  return !Number.isFinite(savedAt) || Date.now() - savedAt > SHARE_TTL_MS;
}

/// Takes the image named by the `?share=` parameter, if any. The entry is
/// deleted after reading (consume once); a missing or stale entry is not
/// an error — the app just starts normally.
export async function consumeSharedImage(): Promise<Blob | null> {
  const params = new URLSearchParams(window.location.search);
  const id = params.get('share');
  if (!id) return null;
  params.delete('share');
  const query = params.toString();
  window.history.replaceState(
    null,
    '',
    window.location.pathname + (query ? `?${query}` : '') + window.location.hash,
  );
  if (!/^[A-Za-z0-9-]+$/.test(id) || !('caches' in window)) return null;
  try {
    const cache = await caches.open(SHARE_CACHE);
    const key = shareKey(id);
    const response = await cache.match(key);
    if (!response) return null;
    const stale = isStale(response);
    const blob = stale ? null : await response.blob();
    await cache.delete(key);
    return blob;
  } catch {
    return null;
  }
}

/// Deletes share entries older than the TTL. Fresh entries — including
/// ones another tab has not consumed yet — are left alone.
export async function sweepSharedImages(): Promise<void> {
  if (!('caches' in window)) return;
  try {
    const cache = await caches.open(SHARE_CACHE);
    for (const request of await cache.keys()) {
      const response = await cache.match(request);
      if (!response || isStale(response)) {
        await cache.delete(request);
      }
    }
  } catch {
    // Sweeping is best-effort housekeeping.
  }
}

/// Fallback decoder for environments where the worker cannot decode:
/// decode on the main thread and hand raw RGBA to the worker instead.
/// EXIF orientation is applied by the browser decoder.
export async function decodeInMainThread(
  blob: Blob,
): Promise<{ rgba: Uint8Array; width: number; height: number }> {
  const bitmap = await createImageBitmap(blob, {
    imageOrientation: 'from-image',
  });
  try {
    const canvas = document.createElement('canvas');
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('no 2d context');
    context.drawImage(bitmap, 0, 0);
    const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
    return {
      rgba: new Uint8Array(imageData.data),
      width: canvas.width,
      height: canvas.height,
    };
  } finally {
    bitmap.close();
  }
}
