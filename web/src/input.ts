// Input normalization: every input route ends as a Blob handed to main.
// M2 wires the file picker (and the drop zone as a click target);
// drag & drop, paste, and share-target intake are added in M3.

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
