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
// same second repeats. There is deliberately no name field: the
// timestamp alone guarantees uniqueness, so naming UI would be
// surface a casual user should not have to think about (decided
// 2026-11-19, same as Voice2QR / MiniMapPrint; if custom naming is
// ever needed, it should come back as a save-time prompt, not a
// standing field).
const handedOutNames = new Set<string>();

export function defaultSaveName(base: string): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return (
    `${base}-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}` +
    `-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}.png`
  );
}

/**
 * The name for one save/share action: a fresh timestamped default,
 * deduplicated within the session.
 */
export function nextSaveName(base: string): string {
  let name = defaultSaveName(base);
  if (handedOutNames.has(name)) {
    const stem = name.slice(0, -'.png'.length);
    let n = 2;
    while (handedOutNames.has(`${stem}-${n}.png`)) n++;
    name = `${stem}-${n}.png`;
  }
  handedOutNames.add(name);
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
