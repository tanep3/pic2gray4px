// Worker client: generation numbers, send control, cancellation.
//
// The WASM conversion inside the worker is synchronous, so while a conversion
// runs the worker's own JS cannot process further messages. Coalescing must
// therefore happen here, on the main side: while a request is in flight we
// keep only the latest queued request and send it when the current one
// settles. Promises of discarded generations are settled as "cancelled"
// (queued ones immediately, the in-flight one when its result arrives) so no
// caller is ever left waiting.

export interface ConvertOptions {
  targetWidth: number | null;
  contrast: number | null;
  unsharp: number | null;
  passes: number;
  diffusion: boolean;
}

export type MainToWorker =
  | { type: 'init'; wasmBase: string }
  | {
      type: 'convert';
      gen: number;
      blob?: Blob;
      rgba?: ArrayBuffer;
      width?: number;
      height?: number;
      options: ConvertOptions;
    };

export type WorkerToMain =
  | { type: 'ready'; workerDecode: boolean }
  | {
      type: 'result';
      gen: number;
      width: number;
      height: number;
      pixels: ArrayBuffer;
      png: ArrayBuffer;
      contrastUsed: number;
      unsharpUsed: number;
      sourceWidth: number;
      sourceHeight: number;
    }
  | { type: 'error'; gen: number; message: string };

export interface WorkerPort {
  postMessage(message: MainToWorker, transfer?: Transferable[]): void;
  addEventListener(
    type: 'message',
    listener: (event: MessageEvent<WorkerToMain>) => void,
  ): void;
}

export interface ConvertRequest {
  blob?: Blob;
  rgba?: ArrayBuffer;
  width?: number;
  height?: number;
  options: ConvertOptions;
}

export type ConvertOutcome =
  | {
      status: 'done';
      gen: number;
      width: number;
      height: number;
      pixels: ArrayBuffer;
      png: ArrayBuffer;
      contrastUsed: number;
      unsharpUsed: number;
      sourceWidth: number;
      sourceHeight: number;
    }
  | { status: 'cancelled'; gen: number }
  | { status: 'error'; gen: number; message: string };

interface PendingEntry {
  gen: number;
  request: ConvertRequest;
  resolve: (outcome: ConvertOutcome) => void;
}

export class ConversionClient {
  private port: WorkerPort;
  private gen = 0;
  private ready = false;
  private readyResolve: ((workerDecode: boolean) => void) | null = null;
  private inFlight: PendingEntry | null = null;
  private queued: PendingEntry | null = null;

  constructor(port: WorkerPort) {
    this.port = port;
    this.port.addEventListener('message', (event) => this.onMessage(event.data));
  }

  init(wasmBase: string): Promise<boolean> {
    const promise = new Promise<boolean>((resolve) => {
      this.readyResolve = resolve;
    });
    this.port.postMessage({ type: 'init', wasmBase });
    return promise;
  }

  convert(request: ConvertRequest): Promise<ConvertOutcome> {
    const gen = ++this.gen;
    if (!this.ready) {
      return Promise.resolve({ status: 'error', gen, message: 'worker not ready' });
    }
    return new Promise<ConvertOutcome>((resolve) => {
      const entry: PendingEntry = { gen, request, resolve };
      if (this.inFlight) {
        // Supersede the queued request: settle it as cancelled right away.
        this.queued?.resolve({ status: 'cancelled', gen: this.queued.gen });
        this.queued = entry;
      } else {
        this.send(entry);
      }
    });
  }

  private send(entry: PendingEntry): void {
    this.inFlight = entry;
    const { request } = entry;
    const transfer: Transferable[] = request.rgba ? [request.rgba] : [];
    this.port.postMessage(
      {
        type: 'convert',
        gen: entry.gen,
        blob: request.blob,
        rgba: request.rgba,
        width: request.width,
        height: request.height,
        options: request.options,
      },
      transfer,
    );
  }

  private onMessage(message: WorkerToMain): void {
    if (message.type === 'ready') {
      this.ready = true;
      this.readyResolve?.(message.workerDecode);
      this.readyResolve = null;
      return;
    }
    const entry = this.inFlight;
    if (!entry || entry.gen !== message.gen) {
      return; // Unknown or already settled generation; nothing to do.
    }
    this.inFlight = null;
    if (entry.gen !== this.gen) {
      // Superseded while running: settle as cancelled, not with its result.
      entry.resolve({ status: 'cancelled', gen: entry.gen });
    } else if (message.type === 'result') {
      entry.resolve({
        status: 'done',
        gen: message.gen,
        width: message.width,
        height: message.height,
        pixels: message.pixels,
        png: message.png,
        contrastUsed: message.contrastUsed,
        unsharpUsed: message.unsharpUsed,
        sourceWidth: message.sourceWidth,
        sourceHeight: message.sourceHeight,
      });
    } else {
      entry.resolve({ status: 'error', gen: message.gen, message: message.message });
    }
    if (this.queued) {
      const next = this.queued;
      this.queued = null;
      this.send(next);
    }
  }
}
