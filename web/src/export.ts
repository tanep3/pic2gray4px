// Output routes. M2: PNG download. Share-sheet output and printing land in M4.

export function savePng(png: ArrayBuffer, filename = 'pic2gray4px.png'): void {
  const blob = new Blob([png], { type: 'image/png' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
