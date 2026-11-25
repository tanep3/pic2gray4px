// Persistence of the conversion panel settings: the last-used
// settings are kept in localStorage and restored on the next visit.
// With nothing stored, the HTML defaults apply (mode: dither, mixer:
// standard, tone curve: normal, size: 576).

const STORAGE_KEY = 'pic2gray4px-settings';

export interface PanelSettings {
  size?: string;
  customWidth?: string;
  mode?: string;
  mixer?: string;
  curve?: string;
  contrastMode?: string;
  contrastValue?: string;
  unsharpMode?: string;
  unsharpValue?: string;
  passes?: string;
  localContrast?: string;
}

const KEYS: Array<keyof PanelSettings> = [
  'size',
  'customWidth',
  'mode',
  'mixer',
  'curve',
  'contrastMode',
  'contrastValue',
  'unsharpMode',
  'unsharpValue',
  'passes',
  'localContrast',
];

export function loadSettings(): PanelSettings | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return null;
    const record = parsed as Record<string, unknown>;
    const settings: PanelSettings = {};
    for (const key of KEYS) {
      const value = record[key];
      if (typeof value === 'string') settings[key] = value;
    }
    return Object.keys(settings).length > 0 ? settings : null;
  } catch {
    return null;
  }
}

export function saveSettings(settings: PanelSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Storage may be unavailable (private mode, quota); persistence
    // is best-effort and never blocks conversion.
  }
}
