import type { Capability, ExportOptions, HtmlOptions, PdfSource, PrintJob, PrintOptions, Printer } from "./options";

/** What a `PdfExport` runs on: the Rust crate, or a memory backend in tests. */
export interface Backend {
  /** Whether the platform has the capability. */
  supports(capability: Capability): boolean;
  /** Renders the source to PDF bytes. */
  createPdf(source: PdfSource, options: ExportOptions): Promise<Uint8Array>;
  /** Renders the source and writes it to `path`. Resolves the path written. */
  savePdf(source: PdfSource, options: ExportOptions, path: string): Promise<string>;
  /**
   * Shows the system's save dialog. Resolves the chosen path, or `null`/`undefined` when the
   * user cancelled.
   */
  pickSavePath(suggestedName: string): Promise<string | null | undefined>;
  /** Renders the HTML to PDF bytes. */
  convertHtml(html: string, options: HtmlOptions): Promise<Uint8Array>;
  /** Sends the PDF to a printer. Resolves the accepted job. */
  printPdf(pdf: Uint8Array, options: PrintOptions): Promise<PrintJob>;
  /** The printers the system knows. */
  listPrinters(): Promise<Printer[]>;
  /** Cancels the running operation, if one is. */
  cancel(): Promise<void>;
}
