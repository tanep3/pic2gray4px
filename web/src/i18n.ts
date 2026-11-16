// Minimal JA/EN dictionary. The initial language follows the device;
// the header switch re-renders every [data-i18n] element.

const ja = {
  language: '言語',
  updateAvailable: '新しいバージョンがあります',
  reload: '再読み込み',
  dropHere: 'ここに画像をドロップ、またはクリックして選択',
  dropSub: '写真を 2×2 セルの白黒パターンに変換します',
  chooseFile: 'ファイルを選ぶ',
  sizeTitle: '出力の幅',
  sizeOriginal: '原画',
  sizeCustom: 'カスタム',
  customWidth: '幅 (px)',
  sourceTitle: '原画',
  resultTitle: '変換結果',
  noImage: '画像がありません',
  noResult: 'まだ変換していません',
  save: 'PNG を保存',
  advanced: '上級者設定',
  contrast: 'コントラスト',
  unsharp: 'シャープネス',
  diffusion: '誤差拡散',
  passes: '最適化パス',
  auto: '自動',
  off: 'オフ',
  manual: '手動',
  loading: '読み込み中…',
  converting: '変換中…',
  done: '変換できました',
  sourceMeta: '{w} × {h} px',
  resultMeta: '{w} × {h} px / contrast {contrast} / unsharp {unsharp}',
  errorPrefix: 'エラー: ',
  fileNotImage: '画像ファイルを選んでください',
  decodeFailed: 'この画像は読み込めませんでした',
};

export type I18nKey = keyof typeof ja;

const en: Record<I18nKey, string> = {
  language: 'Language',
  updateAvailable: 'A new version is available',
  reload: 'Reload',
  dropHere: 'Drop an image here, or click to choose',
  dropSub: 'Convert a photo into a 2×2-cell black-and-white pattern',
  chooseFile: 'Choose a file',
  sizeTitle: 'Output width',
  sizeOriginal: 'Original',
  sizeCustom: 'Custom',
  customWidth: 'Width (px)',
  sourceTitle: 'Original',
  resultTitle: 'Result',
  noImage: 'No image yet',
  noResult: 'Not converted yet',
  save: 'Save PNG',
  advanced: 'Advanced settings',
  contrast: 'Contrast',
  unsharp: 'Sharpness',
  diffusion: 'Error diffusion',
  passes: 'Optimization passes',
  auto: 'Auto',
  off: 'Off',
  manual: 'Manual',
  loading: 'Loading…',
  converting: 'Converting…',
  done: 'Done',
  sourceMeta: '{w} × {h} px',
  resultMeta: '{w} × {h} px / contrast {contrast} / unsharp {unsharp}',
  errorPrefix: 'Error: ',
  fileNotImage: 'Please choose an image file',
  decodeFailed: 'This image could not be read',
};

const dictionaries = { ja, en } as const;

export type Lang = keyof typeof dictionaries;

let current: Lang =
  typeof navigator !== 'undefined' && navigator.language?.toLowerCase().startsWith('en')
    ? 'en'
    : 'ja';

export function getLang(): Lang {
  return current;
}

export function setLang(lang: Lang): void {
  current = lang;
  document.documentElement.lang = lang;
}

export function t(key: I18nKey, vars?: Record<string, string | number>): string {
  let text: string = dictionaries[current][key];
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}

export function applyI18n(): void {
  document.querySelectorAll<HTMLElement>('[data-i18n]').forEach((element) => {
    const key = element.dataset.i18n as I18nKey | undefined;
    if (key && key in dictionaries.ja) {
      element.textContent = t(key);
    }
  });
}
