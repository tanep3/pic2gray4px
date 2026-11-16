// Output routes: PNG download, share-sheet output (Web Share API with
// files — this is how the result reaches printer apps such as Phomemo),
// and printing (the print sheet lives in index.html; main sets its image
// and the physical width before calling window.print()).

export function savePng(png: ArrayBuffer, filename = 'pic2gray4px.png'): void {
  const blob = new Blob([png], { type: 'image/png' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

export function canSharePng(): boolean {
  if (
    typeof navigator === 'undefined' ||
    typeof navigator.share !== 'function' ||
    typeof navigator.canShare !== 'function'
  ) {
    return false;
  }
  try {
    const probe = new File([new Blob(['x'], { type: 'image/png' })], 'probe.png', {
      type: 'image/png',
    });
    return navigator.canShare({ files: [probe] });
  } catch {
    return false;
  }
}

export type ShareOutcome = 'shared' | 'cancelled' | 'unsupported';

export async function sharePng(
  png: ArrayBuffer,
  filename = 'pic2gray4px.png',
): Promise<ShareOutcome> {
  const file = new File([png], filename, { type: 'image/png' });
  if (!navigator.canShare?.({ files: [file] })) return 'unsupported';
  try {
    await navigator.share({ files: [file] });
    return 'shared';
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      return 'cancelled'; // The user closed the share sheet; not an error.
    }
    throw error;
  }
}
