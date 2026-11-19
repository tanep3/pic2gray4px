// Output routes: PNG download, share-sheet output (Web Share API with
// files — this is how the result reaches printer apps such as Phomemo),
// and printing (the print sheet lives in index.html; main sets its image
// and the physical width before calling window.print()).

// ----- save file names -----
// A web app cannot see what already exists at the save destination
// (the Downloads folder, or the app a share is handed to), so a fixed
// file name silently overwrites the previous result saved there. The
// default name therefore carries a to-the-second timestamp, making
// collisions impossible by construction; names handed out in this
// session are also remembered and get a -2, -3, ... suffix if the
// same second repeats. A name the user typed is used verbatim:
// picking a name is their intent, overwrite included. (Same rule as
// Voice2QR / MiniMapPrint, 2026-11-19.)
const handedOutNames = new Set<string>();

export function defaultSaveName(base: string): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return (
    `${base}-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}` +
    `-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}.png`
  );
}

export function sanitizeSaveName(raw: string): string {
  const name = raw
    .trim()
    .replace(/[\\/:*?"<>|]/g, '_')
    .replace(/\s+/g, ' ')
    .replace(/^\.+/, '')
    .slice(0, 100);
  if (!name) return '';
  return /\.png$/i.test(name) ? name : `${name}.png`;
}

/**
 * The name for one save/share action. While the field is untouched,
 * a fresh unique default is generated (and shown in the field). Once
 * the user edits the field, their text wins as-is.
 */
export function resolveSaveName(
  input: HTMLInputElement,
  edited: boolean,
  base: string,
): string {
  if (edited) {
    const typed = sanitizeSaveName(input.value);
    if (typed) return typed;
  }
  let name = defaultSaveName(base);
  if (handedOutNames.has(name)) {
    const stem = name.slice(0, -'.png'.length);
    let n = 2;
    while (handedOutNames.has(`${stem}-${n}.png`)) n++;
    name = `${stem}-${n}.png`;
  }
  handedOutNames.add(name);
  input.value = name;
  return name;
}

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
