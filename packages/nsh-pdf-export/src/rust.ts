import { NativePdfExport } from "nsh-pdf-export/native";
import type { Backend } from "./backend";
import { asPdfExportError } from "./errors";
import type { Capability, ExportOptions, HtmlOptions, PdfSource, PrintJob, PrintOptions, Printer } from "./options";

/**
 * The Rust crate's PDF export (`nsh-pdf-export/native`). The app or package must list the crate
 * under `"rust"` in `nsh.json`; importing this module without it fails the bundle.
 */
export function rustBackend(): Backend {
  return new RustBackend();
}

class RustBackend implements Backend {
  private readonly native = new NativePdfExport();

  supports(capability: Capability): boolean {
    return this.native.capabilities().includes(capability);
  }

  createPdf(source: PdfSource, options: ExportOptions): Promise<Uint8Array> {
    return this.settle(async () => decodeBase64(await this.native.createPdf(source, options)));
  }

  savePdf(source: PdfSource, options: ExportOptions, path: string): Promise<string> {
    return this.settle(() => this.native.savePdf(source, options, path));
  }

  pickSavePath(suggestedName: string): Promise<string | null | undefined> {
    return this.settle(() => this.native.pickSavePath(suggestedName));
  }

  convertHtml(html: string, options: HtmlOptions): Promise<Uint8Array> {
    return this.settle(async () => decodeBase64(await this.native.convertHtml(html, options)));
  }

  printPdf(pdf: Uint8Array, options: PrintOptions): Promise<PrintJob> {
    return this.settle(async () => this.native.printPdf(encodeBase64(pdf), options));
  }

  listPrinters(): Promise<Printer[]> {
    return this.settle(async () => this.native.listPrinters());
  }

  cancel(): Promise<void> {
    // The crate answers whether a cancel landed; the backend's contract needs no answer.
    return this.settle(async () => {
      await this.native.cancel();
    });
  }

  private async settle<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await operation();
    } catch (error) {
      throw asPdfExportError(error);
    }
  }
}

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const VALUES = new Map<string, number>([...ALPHABET].map((character, value) => [character, value]));

/** Standard base64, as the crate's `createPdf` and `convertHtml` hand their bytes over. */
function encodeBase64(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i];
    const b1 = i + 1 < bytes.length ? bytes[i + 1] : 0;
    const b2 = i + 2 < bytes.length ? bytes[i + 2] : 0;
    out += ALPHABET[b0 >> 2];
    out += ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)];
    out += i + 1 < bytes.length ? ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] : "=";
    out += i + 2 < bytes.length ? ALPHABET[b2 & 0x3f] : "=";
  }
  return out;
}

/** The base64 the crate hands back, as bytes. Padding stops the input; characters outside the alphabet are skipped. */
function decodeBase64(text: string): Uint8Array {
  const digits: number[] = [];
  for (const character of text) {
    if (character === "=") break;
    const value = VALUES.get(character);
    if (value !== undefined) digits.push(value);
  }
  const bytes = new Uint8Array(Math.floor(digits.length * 3 / 4));
  let at = 0;
  const emit = (d0: number, d1: number, d2: number, d3: number, count: number) => {
    if (count > 0) bytes[at++] = (d0 << 2) | (d1 >> 4);
    if (count > 1) bytes[at++] = ((d1 & 0x0f) << 4) | (d2 >> 2);
    if (count > 2) bytes[at++] = ((d2 & 0x03) << 6) | d3;
  };
  let i = 0;
  for (; i + 4 <= digits.length; i += 4) emit(digits[i], digits[i + 1], digits[i + 2], digits[i + 3], 3);
  const rest = digits.length - i;
  if (rest === 2) emit(digits[i], digits[i + 1], 0, 0, 1);
  else if (rest === 3) emit(digits[i], digits[i + 1], digits[i + 2], 0, 2);
  return bytes.subarray(0, at);
}
