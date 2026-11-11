import type { Backend } from "./backend";
import { pdfExportError, type PdfExportErrorCode } from "./errors";
import type { Capability, ExportOptions, HtmlOptions, PdfSource, PrintJob, PrintOptions, Printer } from "./options";

/** What a scripted call answers: the value, or `{ error: code }` to reject with it. */
export type ScriptedAnswer<T> = T | { error: PdfExportErrorCode };

/**
 * The answers a memory backend gives. Each method shifts its own queue; an empty queue answers
 * the method's default. Every answer comes after `delayMs`, even when the caller has given up.
 */
export interface MemoryBackendConfig {
  /** What `supports` answers. The default is every capability. */
  capabilities?: Capability[];
  /** Milliseconds before each answer. The default is immediate. */
  delayMs?: number;
  createPdf?: ScriptedAnswer<Uint8Array>[];
  savePdf?: ScriptedAnswer<string>[];
  /** `null` answers as a user who cancelled the dialog. */
  pickSavePath?: ScriptedAnswer<string | null>[];
  convertHtml?: ScriptedAnswer<Uint8Array>[];
  printPdf?: ScriptedAnswer<PrintJob>[];
  listPrinters?: ScriptedAnswer<Printer[]>[];
}

/** One call the backend was asked, with what it was asked with. */
export type MemoryCall =
  | { method: "supports"; capability: Capability }
  | { method: "createPdf"; source: PdfSource; options: ExportOptions }
  | { method: "savePdf"; source: PdfSource; options: ExportOptions; path: string }
  | { method: "pickSavePath"; suggestedName: string }
  | { method: "convertHtml"; html: string; options: HtmlOptions }
  | { method: "printPdf"; pdf: Uint8Array; options: PrintOptions }
  | { method: "listPrinters" }
  | { method: "cancel" };

/** A backend the test drives: its answers are scripted, and every call is logged. */
export interface MemoryBackend extends Backend {
  /** Every call so far, in the order they came. */
  calls(): readonly MemoryCall[];
}

/** The bytes the memory backend hands out as a PDF: the header of one, and nothing else. */
const FAKE_PDF = new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34]);

/** No answer value carries an `error` field, so an object with a string one is a scripted code. */
function isFailure(value: unknown): value is { error: PdfExportErrorCode } {
  if (typeof value !== "object" || value === null || !("error" in value)) return false;
  return typeof (value as Record<"error", unknown>).error === "string";
}

/** A memory backend, for tests and for apps that run without a platform behind them. */
export function memoryBackend(config: MemoryBackendConfig = {}): MemoryBackend {
  const capabilities = new Set(config.capabilities ?? [
    "createPdf",
    "saveToPath",
    "saveDialog",
    "convertHtml",
    "print",
    "listPrinters",
  ]);
  const delayMs = config.delayMs ?? 0;
  const queues = {
    createPdf: [...(config.createPdf ?? [])],
    savePdf: [...(config.savePdf ?? [])],
    pickSavePath: [...(config.pickSavePath ?? [])],
    convertHtml: [...(config.convertHtml ?? [])],
    printPdf: [...(config.printPdf ?? [])],
    listPrinters: [...(config.listPrinters ?? [])],
  };
  const calls: MemoryCall[] = [];
  let jobs = 0;

  const answer = <T>(queue: ScriptedAnswer<T>[], fallback: () => T): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      setTimeout(() => {
        const scripted = queue.length > 0 ? queue.shift() : undefined;
        if (scripted === undefined) {
          resolve(fallback());
        } else if (isFailure(scripted)) {
          reject(pdfExportError(scripted.error));
        } else {
          resolve(scripted);
        }
      }, delayMs);
    });

  return {
    supports: (capability) => {
      calls.push({ method: "supports", capability });
      return capabilities.has(capability);
    },
    createPdf: (source, options) => {
      calls.push({ method: "createPdf", source, options });
      return answer(queues.createPdf, () => FAKE_PDF);
    },
    savePdf: (source, options, path) => {
      calls.push({ method: "savePdf", source, options, path });
      return answer(queues.savePdf, () => path);
    },
    pickSavePath: (suggestedName) => {
      calls.push({ method: "pickSavePath", suggestedName });
      return answer(queues.pickSavePath, () => suggestedName);
    },
    convertHtml: (html, options) => {
      calls.push({ method: "convertHtml", html, options });
      return answer(queues.convertHtml, () => FAKE_PDF);
    },
    printPdf: (pdf, options) => {
      calls.push({ method: "printPdf", pdf, options });
      return answer(queues.printPdf, () => {
        jobs += 1;
        return { id: `memory-${jobs}` };
      });
    },
    listPrinters: () => {
      calls.push({ method: "listPrinters" });
      return answer(queues.listPrinters, () => [{ name: "Memory Printer", isDefault: true }]);
    },
    cancel: async () => {
      calls.push({ method: "cancel" });
    },
    calls: () => calls,
  };
}
