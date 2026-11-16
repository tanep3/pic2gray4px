// Conversion worker: WASM initialization, decoding, conversion, PNG encoding.
// Requests arrive strictly one at a time (the client's send control
// guarantees this), so the worker simply processes them serially.

import { planConversion } from './client';
import type { MainToWorker, WorkerToMain } from './client';

interface ConvertResultHandle {
  readonly width: number;
  readonly height: number;
  readonly pixels: Uint8Array;
  readonly contrast_used: number;
  readonly unsharp_used: number;
}

interface WasmGlue {
  default: (arg: { module_or_path: string }) => Promise<unknown>;
  convert_rgba: (
    rgba: Uint8Array,
    width: number,
    height: number,
    targetWidth: number | null,
    contrast: number | null,
    unsharp: number | null,
    passes: number,
    diffusion: boolean,
  ) => ConvertResultHandle;
  encode_1bit_png: (width: number, height: number, pixels: Uint8Array) => Uint8Array;
}

interface WorkerScope {
  onmessage: ((event: MessageEvent<MainToWorker>) => void) | null;
  postMessage: (message: WorkerToMain, transfer?: Transferable[]) => void;
}

const scope = self as unknown as WorkerScope;

let glue: WasmGlue | null = null;
let workerDecode = false;

scope.onmessage = (event) => {
  void handle(event.data);
};

async function handle(message: MainToWorker): Promise<void> {
  if (message.type === 'init') {
    try {
      // The glue URL is resolved from the page base (document.baseURI on the
      // main side), never relative to this worker module (MiniMapPrint lesson).
      const loaded = (await import(
        /* @vite-ignore */ `${message.wasmBase}pic2gray4px_wasm.js`
      )) as WasmGlue;
      await loaded.default({
        module_or_path: `${message.wasmBase}pic2gray4px_wasm_bg.wasm`,
      });
      glue = loaded;
      workerDecode =
        typeof createImageBitmap === 'function' &&
        typeof OffscreenCanvas === 'function';
      scope.postMessage({ type: 'ready', workerDecode });
    } catch (error) {
      // No generation exists yet; report against gen 0 and stay not-ready.
      scope.postMessage({
        type: 'error',
        gen: 0,
        message: `wasm init failed: ${String(error)}`,
      });
    }
    return;
  }

  const gen = message.gen;
  try {
    if (!glue) throw new Error('worker not initialized');
    let rgba: Uint8Array;
    let rgbaWidth: number;
    let rgbaHeight: number;
    let sourceWidth: number;
    let sourceHeight: number;
    if (message.blob) {
      if (!workerDecode) throw new Error('worker decode unavailable');
      const decoded = await decodeBlob(message.blob, message.options.targetWidth);
      rgba = decoded.rgba;
      rgbaWidth = decoded.width;
      rgbaHeight = decoded.height;
      sourceWidth = decoded.sourceWidth;
      sourceHeight = decoded.sourceHeight;
    } else if (message.rgba && message.width && message.height) {
      rgba = new Uint8Array(message.rgba);
      rgbaWidth = message.width;
      rgbaHeight = message.height;
      sourceWidth = message.sourceWidth ?? message.width;
      sourceHeight = message.sourceHeight ?? message.height;
    } else {
      throw new Error('convert request has neither blob nor rgba');
    }
    const { options } = message;
    // The plan is recomputed here from the true source size so both
    // decode routes agree on the effective target width and the cap.
    const plan = planConversion(sourceWidth, sourceHeight, options.targetWidth);
    const result = glue.convert_rgba(
      rgba,
      rgbaWidth,
      rgbaHeight,
      plan.targetWidth,
      options.contrast,
      options.unsharp,
      options.passes,
      options.diffusion,
    );
    const pixels = result.pixels;
    const png = glue.encode_1bit_png(result.width, result.height, pixels);
    const pixelsBuffer = pixels.buffer as ArrayBuffer;
    const pngBuffer = png.buffer as ArrayBuffer;
    scope.postMessage(
      {
        type: 'result',
        gen,
        width: result.width,
        height: result.height,
        pixels: pixelsBuffer,
        png: pngBuffer,
        contrastUsed: result.contrast_used,
        unsharpUsed: result.unsharp_used,
        sourceWidth,
        sourceHeight,
        capped: plan.capped,
      },
      [pixelsBuffer, pngBuffer],
    );
  } catch (error) {
    scope.postMessage({
      type: 'error',
      gen,
      message: error instanceof Error ? error.message : String(error),
    });
  }
}

async function decodeBlob(
  blob: Blob,
  targetWidth: number | null,
): Promise<{
  rgba: Uint8Array;
  width: number;
  height: number;
  sourceWidth: number;
  sourceHeight: number;
}> {
  const bitmap = await createImageBitmap(blob, {
    imageOrientation: 'from-image',
  });
  try {
    // Decode at the planned size: when the conversion is capped, the
    // full-size canvas (and its RGBA copy) never exists at all.
    const plan = planConversion(bitmap.width, bitmap.height, targetWidth);
    const canvas = new OffscreenCanvas(plan.decodeWidth, plan.decodeHeight);
    const context = canvas.getContext('2d');
    if (!context) throw new Error('no 2d context in worker');
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
    return {
      rgba: new Uint8Array(imageData.data),
      width: canvas.width,
      height: canvas.height,
      sourceWidth: bitmap.width,
      sourceHeight: bitmap.height,
    };
  } finally {
    bitmap.close();
  }
}
