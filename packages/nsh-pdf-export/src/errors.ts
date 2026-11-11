/** The codes a rejection carries. Apps match on `code`. */
export type PdfExportErrorCode =
  | "ERR_PDF_EXPORT_NOT_AVAILABLE"
  | "ERR_PDF_EXPORT_NOT_SUPPORTED"
  | "ERR_PDF_EXPORT_INVALID_ARGUMENT"
  | "ERR_PDF_EXPORT_CANCELLED"
  | "ERR_PDF_EXPORT_TIMEOUT"
  | "ERR_PDF_EXPORT_IN_PROGRESS"
  | "ERR_PDF_EXPORT_FONT_NOT_FOUND"
  | "ERR_PDF_EXPORT_FONT_INVALID"
  | "ERR_PDF_EXPORT_WRITE_FAILED"
  | "ERR_PDF_EXPORT_NO_PRINTER"
  | "ERR_PDF_EXPORT_PRINT_FAILED"
  | "ERR_PDF_EXPORT_BACKEND";

/** Same wording as the Rust crate's messages. None holds user data. */
const MESSAGES: Record<PdfExportErrorCode, string> = {
  ERR_PDF_EXPORT_NOT_AVAILABLE: "PDF export is not available on this platform.",
  ERR_PDF_EXPORT_NOT_SUPPORTED: "This capability is not supported here yet.",
  ERR_PDF_EXPORT_INVALID_ARGUMENT: "An argument is empty or not allowed on this platform.",
  ERR_PDF_EXPORT_CANCELLED: "The operation was cancelled.",
  ERR_PDF_EXPORT_TIMEOUT: "The operation timed out. Try again.",
  ERR_PDF_EXPORT_IN_PROGRESS: "Another export or print is already running. Wait for it to finish.",
  ERR_PDF_EXPORT_FONT_NOT_FOUND: "The requested font is not available.",
  ERR_PDF_EXPORT_FONT_INVALID: "The font bytes could not be read.",
  ERR_PDF_EXPORT_WRITE_FAILED: "The file could not be written.",
  ERR_PDF_EXPORT_NO_PRINTER: "No printer is available.",
  ERR_PDF_EXPORT_PRINT_FAILED: "The print job failed.",
  ERR_PDF_EXPORT_BACKEND: "The platform's PDF export service failed.",
};

const KNOWN = new Set<string>(Object.keys(MESSAGES));

/** A rejection of the PDF export API. `code` is the stable name; the message says what to do. */
export class PdfExportError extends Error {
  readonly code: PdfExportErrorCode;

  constructor(code: PdfExportErrorCode) {
    super(MESSAGES[code]);
    this.name = "PdfExportError";
    this.code = code;
  }
}

/** The error for a known code. */
export function pdfExportError(code: PdfExportErrorCode): PdfExportError {
  return new PdfExportError(code);
}

/**
 * Turns whatever the native side or a host threw into a coded `PdfExportError`. A host's
 * rejection carries `ERR_NSH_HOST_ERROR`, and its message starts with the code, as
 * `ERR_PDF_EXPORT_CANCELLED: text`. Anything unknown is a backend failure.
 */
export function asPdfExportError(error: unknown): PdfExportError {
  if (error instanceof PdfExportError) return error;
  const raw = (error ?? {}) as { code?: unknown; message?: unknown };
  const code = typeof raw.code === "string" ? raw.code : "";
  const message = typeof raw.message === "string" ? raw.message : "";
  if (code === "ERR_NSH_HOST_ERROR") {
    const named = /^(ERR_PDF_EXPORT_[A-Z_]+): /.exec(message)?.[1];
    return named !== undefined && KNOWN.has(named)
      ? pdfExportError(named as PdfExportErrorCode)
      : pdfExportError("ERR_PDF_EXPORT_BACKEND");
  }
  return KNOWN.has(code) ? pdfExportError(code as PdfExportErrorCode) : pdfExportError("ERR_PDF_EXPORT_BACKEND");
}
