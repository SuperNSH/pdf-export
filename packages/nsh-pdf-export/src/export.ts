import type { Backend } from "./backend";
import { asPdfExportError, pdfExportError, type PdfExportError } from "./errors";
import { rustBackend } from "./rust";
import type { Capability, ExportOptions, HtmlOptions, PageSize, PdfSource, PrintJob, PrintOptions, Printer } from "./options";

type Outcome<T> = { ok: true; value: T } | { ok: false; error: PdfExportError };

/** The running operation: what a `cancel` settles it with. */
interface Running {
  cancel(error: PdfExportError): void;
}

/** The bound every operation resolves within, unless `withTimeoutMs` moves it. */
const DEFAULT_TIMEOUT_MS = 30_000;

/** The bound on telling the backend to stop, mirroring the Rust crate's cancel bound. */
const CANCEL_TIMEOUT_MS = 1_000;

/**
 * Creating PDFs and printing them, on the platform's own terms. It runs on the Rust crate
 * unless it is given another backend. One operation runs at a time, each resolves exactly
 * once, and none outlives its bound.
 */
export class PdfExport {
  private readonly backend: Backend;
  private timeoutMs = DEFAULT_TIMEOUT_MS;
  private running: Running | null = null;

  constructor(backend?: Backend) {
    this.backend = backend ?? rustBackend();
  }

  /** Moves the bound every operation resolves within. A bound is zero milliseconds or more. */
  withTimeoutMs(timeoutMs: number): this {
    if (!(timeoutMs >= 0) || !Number.isFinite(timeoutMs)) {
      throw pdfExportError("ERR_PDF_EXPORT_INVALID_ARGUMENT");
    }
    this.timeoutMs = timeoutMs;
    return this;
  }

  /** Whether the platform has the capability. */
  supports(capability: Capability): boolean {
    return this.backend.supports(capability);
  }

  /** Renders the source to PDF bytes. */
  createPdf(source: PdfSource, options: ExportOptions = {}): Promise<Uint8Array> {
    return this.start(sourceError(source, options), () => this.backend.createPdf(source, options));
  }

  /** Renders the source and writes it to `path`. Resolves the path written. */
  savePdf(source: PdfSource, options: ExportOptions = {}, path: string): Promise<string> {
    return this.start(sourceError(source, options), () => this.backend.savePdf(source, options, path));
  }

  /**
   * Renders the source and writes it where the system's save dialog sends it. A dialog the
   * user cancelled rejects with `ERR_PDF_EXPORT_CANCELLED`. Resolves the path written.
   */
  savePdfWithDialog(source: PdfSource, options: ExportOptions = {}, suggestedName?: string): Promise<string> {
    return this.start(sourceError(source, options), async (backend) => {
      const picked = await backend.pickSavePath(suggestedName ?? "document.pdf");
      if (picked === null || picked === undefined) {
        throw pdfExportError("ERR_PDF_EXPORT_CANCELLED");
      }
      return backend.savePdf(source, options, picked);
    });
  }

  /** Renders the HTML to PDF bytes. */
  convertHtml(html: string, options: HtmlOptions = {}): Promise<Uint8Array> {
    const error = html.trim() === "" ? pdfExportError("ERR_PDF_EXPORT_INVALID_ARGUMENT") : null;
    return this.start(error, () => this.backend.convertHtml(html, options));
  }

  /** Sends the PDF to a printer. Resolves the accepted job. */
  printPdf(pdf: Uint8Array, options: PrintOptions = {}): Promise<PrintJob> {
    const copies = options.copies ?? 1;
    const error =
      Number.isInteger(copies) && copies >= 1 && copies <= 999
        ? null
        : pdfExportError("ERR_PDF_EXPORT_INVALID_ARGUMENT");
    return this.start(error, () => this.backend.printPdf(pdf, options));
  }

  /** The printers the system knows. */
  listPrinters(): Promise<Printer[]> {
    return this.operation((backend) => backend.listPrinters());
  }

  /**
   * Cancels the running operation: it rejects with `ERR_PDF_EXPORT_CANCELLED` and this
   * resolves. With nothing running it rejects with `ERR_PDF_EXPORT_IN_PROGRESS`.
   */
  async cancel(): Promise<void> {
    const running = this.running;
    if (running === null) {
      throw pdfExportError("ERR_PDF_EXPORT_IN_PROGRESS");
    }
    running.cancel(pdfExportError("ERR_PDF_EXPORT_CANCELLED"));
    // The backend is told to stop, but its answer — or its failure — no longer matters.
    await attemptCancel(this.backend);
  }

  /**
   * Runs the only operation allowed at once. A second call while one runs rejects with
   * `ERR_PDF_EXPORT_IN_PROGRESS`; the running one is unaffected. On the bound's expiry the
   * call rejects with `ERR_PDF_EXPORT_TIMEOUT`, a cancel is attempted on the backend, and
   * the gate opens for the next call — the backend's late answer lands on a settled call.
   */
  private operation<T>(invoke: (backend: Backend) => Promise<T>): Promise<T> {
    if (this.running !== null) {
      return Promise.reject(pdfExportError("ERR_PDF_EXPORT_IN_PROGRESS"));
    }
    return new Promise<T>((resolve, reject) => {
      let settled = false;
      let timer: ReturnType<typeof setTimeout> | null = null;
      const running: Running = { cancel: (error) => finish({ ok: false, error }) };
      const finish = (outcome: Outcome<T>) => {
        if (settled) return;
        settled = true;
        if (timer !== null) clearTimeout(timer);
        if (this.running === running) this.running = null;
        if (outcome.ok) resolve(outcome.value);
        else reject(outcome.error);
      };
      this.running = running;
      timer = setTimeout(() => {
        if (this.running === running) this.running = null;
        finish({ ok: false, error: pdfExportError("ERR_PDF_EXPORT_TIMEOUT") });
        void attemptCancel(this.backend);
      }, this.timeoutMs);
      let invoked: Promise<T>;
      try {
        invoked = invoke(this.backend);
      } catch (error: unknown) {
        // A backend may throw before returning its promise; that rejects the call like any failure.
        invoked = Promise.reject(error);
      }
      invoked.then(
        (value) => finish({ ok: true, value }),
        (error: unknown) => finish({ ok: false, error: asPdfExportError(error) }),
      );
    });
  }

  /** A rejection for an invalid argument wins over the one-operation gate. */
  private start<T>(error: PdfExportError | null, invoke: (backend: Backend) => Promise<T>): Promise<T> {
    return error !== null ? Promise.reject(error) : this.operation(invoke);
  }
}

/** v1 lays out text only, a document needs a page, and a page needs positive, finite sides. */
function sourceError(source: PdfSource, options: ExportOptions): PdfExportError | null {
  if (source.kind !== "text" || source.pages.length === 0 || badSize(options.pageSize)) {
    return pdfExportError("ERR_PDF_EXPORT_INVALID_ARGUMENT");
  }
  return null;
}

/**
 * Tells the backend to stop and gives up on its answer after a second. The attempt must
 * never hold a caller's answer: a backend that ignores cancel only delays cleanup.
 */
async function attemptCancel(backend: Backend): Promise<void> {
  let attempt: Promise<void>;
  try {
    attempt = backend.cancel();
  } catch {
    // A backend may throw before returning its promise; the attempt is over either way.
    attempt = Promise.resolve();
  }
  let timer: ReturnType<typeof setTimeout> | null = null;
  await Promise.race([
    attempt.catch(() => {}),
    new Promise<void>((resolve) => {
      timer = setTimeout(() => resolve(undefined), CANCEL_TIMEOUT_MS);
    }),
  ]);
  if (timer !== null) clearTimeout(timer);
}

function badSize(size: PageSize | undefined): boolean {
  if (size === undefined || typeof size === "string") return false;
  return !positive(size.widthMm) || !positive(size.heightMm);
}

function positive(value: number): boolean {
  return Number.isFinite(value) && value > 0;
}
