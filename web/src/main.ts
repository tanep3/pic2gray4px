// Screen conductor: state machine (idle / converting / done / error),
// option collection, and reflecting only the latest generation's result.

import {
  ConversionClient,
  type ConvertOutcome,
  type ConvertOptions,
  type ConvertRequest,
  type WorkerPort,
} from './client';
import { savePng } from './export';
import { applyI18n, getLang, setLang, t, type Lang } from './i18n';
import { decodeInMainThread, wireFilePicker } from './input';
import { checkForUpdate, registerServiceWorker } from './pwa';

function el<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`missing element #${id}`);
  return element as T;
}

const langSelect = el<HTMLSelectElement>('lang-select');
const updateNotice = el<HTMLElement>('update-notice');
const statusLine = el<HTMLElement>('status-line');
const sourceImg = el<HTMLImageElement>('source-img');
const sourceEmpty = el<HTMLElement>('source-empty');
const sourceMeta = el<HTMLElement>('source-meta');
const resultImg = el<HTMLImageElement>('result-img');
const resultEmpty = el<HTMLElement>('result-empty');
const resultMeta = el<HTMLElement>('result-meta');
const saveButton = el<HTMLButtonElement>('save-button');
const customWidthRow = el<HTMLElement>('custom-width-row');
const customWidthInput = el<HTMLInputElement>('custom-width');

let client: ConversionClient | null = null;
let workerDecode = false;
let currentBlob: Blob | null = null;
let sourceUrl: string | null = null;
let resultUrl: string | null = null;
let resultPng: ArrayBuffer | null = null;
let lastResult: { width: number; height: number; contrastUsed: number; unsharpUsed: number } | null =
  null;

function setStatus(text: string, isError = false): void {
  statusLine.textContent = text;
  statusLine.classList.toggle('error', isError);
}

function formatValue(v: number): string {
  return String(Math.round(v * 100) / 100);
}

function selectedSize(): string {
  const pressed = document.querySelector<HTMLButtonElement>(
    '#size-chips .chip[aria-pressed="true"]',
  );
  return pressed?.dataset.size ?? 'original';
}

function adjustmentValue(
  modeId: string,
  rangeId: string,
  offValue: number,
): number | null {
  const mode = el<HTMLSelectElement>(modeId).value;
  if (mode === 'auto') return null;
  if (mode === 'off') return offValue;
  return Number.parseFloat(el<HTMLInputElement>(rangeId).value);
}

function collectOptions(): ConvertOptions {
  const size = selectedSize();
  let targetWidth: number | null = null;
  if (size === 'custom') {
    const parsed = Number.parseInt(customWidthInput.value, 10);
    targetWidth = Number.isFinite(parsed) && parsed >= 1 ? parsed : null;
  } else if (size !== 'original') {
    targetWidth = Number.parseInt(size, 10);
  }
  return {
    targetWidth,
    contrast: adjustmentValue('contrast-mode', 'contrast-range', 1.0),
    unsharp: adjustmentValue('unsharp-mode', 'unsharp-range', 0.0),
    passes: Number.parseInt(el<HTMLInputElement>('passes-range').value, 10),
    diffusion: el<HTMLInputElement>('diffusion-check').checked,
  };
}

async function requestConvert(): Promise<void> {
  if (!client || !currentBlob) return;
  const options = collectOptions();
  setStatus(t('converting'));
  let request: ConvertRequest;
  if (workerDecode) {
    request = { blob: currentBlob, options };
  } else {
    try {
      const decoded = await decodeInMainThread(currentBlob);
      request = {
        rgba: decoded.rgba.buffer as ArrayBuffer,
        width: decoded.width,
        height: decoded.height,
        options,
      };
    } catch {
      setStatus(t('errorPrefix') + t('decodeFailed'), true);
      return;
    }
  }
  const outcome: ConvertOutcome = await client.convert(request);
  if (outcome.status === 'cancelled') {
    return; // A newer generation is already in flight; it will report.
  }
  if (outcome.status === 'error') {
    setStatus(t('errorPrefix') + outcome.message, true);
    return;
  }
  lastResult = {
    width: outcome.width,
    height: outcome.height,
    contrastUsed: outcome.contrastUsed,
    unsharpUsed: outcome.unsharpUsed,
  };
  resultPng = outcome.png;
  if (resultUrl) URL.revokeObjectURL(resultUrl);
  resultUrl = URL.createObjectURL(new Blob([outcome.png], { type: 'image/png' }));
  resultImg.src = resultUrl;
  resultImg.hidden = false;
  resultEmpty.hidden = true;
  renderResultMeta();
  saveButton.disabled = false;
  setStatus(t('done'));
}

function renderResultMeta(): void {
  if (!lastResult) return;
  resultMeta.textContent = t('resultMeta', {
    w: lastResult.width,
    h: lastResult.height,
    contrast: formatValue(lastResult.contrastUsed),
    unsharp: formatValue(lastResult.unsharpUsed),
  });
}

function onBlob(blob: Blob): void {
  currentBlob = blob;
  if (sourceUrl) URL.revokeObjectURL(sourceUrl);
  sourceUrl = URL.createObjectURL(blob);
  sourceImg.src = sourceUrl;
  sourceImg.hidden = false;
  sourceEmpty.hidden = true;
  sourceImg.onload = () => {
    sourceMeta.textContent = t('sourceMeta', {
      w: sourceImg.naturalWidth,
      h: sourceImg.naturalHeight,
    });
  };
  void requestConvert();
}

function initSizeChips(): void {
  document.querySelectorAll<HTMLButtonElement>('#size-chips .chip').forEach((chip) => {
    chip.addEventListener('click', () => {
      document
        .querySelectorAll<HTMLButtonElement>('#size-chips .chip')
        .forEach((c) => c.setAttribute('aria-pressed', 'false'));
      chip.setAttribute('aria-pressed', 'true');
      customWidthRow.hidden = chip.dataset.size !== 'custom';
      void requestConvert();
    });
  });
  customWidthInput.addEventListener('change', () => void requestConvert());
}

function initAdvanced(): void {
  const pairs: Array<[string, string, string]> = [
    ['contrast-mode', 'contrast-range', 'contrast-value'],
    ['unsharp-mode', 'unsharp-range', 'unsharp-value'],
  ];
  for (const [modeId, rangeId, valueId] of pairs) {
    const mode = el<HTMLSelectElement>(modeId);
    const range = el<HTMLInputElement>(rangeId);
    const value = el<HTMLElement>(valueId);
    const sync = () => {
      range.disabled = mode.value !== 'manual';
      value.textContent = formatValue(Number.parseFloat(range.value));
    };
    mode.addEventListener('change', () => {
      sync();
      void requestConvert();
    });
    range.addEventListener('input', () => {
      value.textContent = formatValue(Number.parseFloat(range.value));
    });
    range.addEventListener('change', () => void requestConvert());
    sync();
  }
  const passes = el<HTMLInputElement>('passes-range');
  const passesValue = el<HTMLElement>('passes-value');
  passesValue.textContent = passes.value;
  passes.addEventListener('input', () => {
    passesValue.textContent = passes.value;
  });
  passes.addEventListener('change', () => void requestConvert());
  el<HTMLInputElement>('diffusion-check').addEventListener('change', () =>
    void requestConvert(),
  );
}

async function init(): Promise<void> {
  setLang(getLang());
  langSelect.value = getLang();
  applyI18n();
  langSelect.addEventListener('change', () => {
    setLang(langSelect.value as Lang);
    applyI18n();
    renderResultMeta();
  });

  wireFilePicker({
    input: el<HTMLInputElement>('file-input'),
    zone: el<HTMLElement>('drop-zone'),
    button: el<HTMLButtonElement>('file-button'),
    onBlob,
    onInvalid: () => setStatus(t('errorPrefix') + t('fileNotImage'), true),
  });
  initSizeChips();
  initAdvanced();
  saveButton.addEventListener('click', () => {
    if (resultPng) savePng(resultPng);
  });
  el<HTMLButtonElement>('update-reload').addEventListener('click', () =>
    window.location.reload(),
  );

  registerServiceWorker();
  void checkForUpdate(() => {
    updateNotice.hidden = false;
  });

  setStatus(t('loading'));
  const worker = new Worker(new URL('./worker.ts', import.meta.url), {
    type: 'module',
  });
  client = new ConversionClient(worker as unknown as WorkerPort);
  const wasmBase = new URL('wasm/', document.baseURI).href;
  workerDecode = await client.init(wasmBase);
  setStatus('');
}

void init();
