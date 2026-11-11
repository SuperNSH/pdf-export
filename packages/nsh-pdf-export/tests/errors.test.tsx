import { expect, test } from "nsh/test";
import {
  asPdfExportError,
  memoryBackend,
  PdfExport,
  pdfExportError,
  type PdfExportErrorCode,
  type PdfExportError,
  type PdfSource,
} from "../src";

function source(): PdfSource {
  return { kind: "text", title: "Report", pages: [{ lines: ["a line of text"] }] };
}

/** The coded error a call rejects with, so its message can be checked. */
async function rejection(promise: Promise<unknown>): Promise<PdfExportError> {
  try {
    await promise;
  } catch (error: unknown) {
    return asPdfExportError(error);
  }
  throw new Error("the call must reject");
}

// The contract's table: every code carries this message, word for word.
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

test("every code has its message", () => {
  for (const [code, message] of Object.entries(MESSAGES)) {
    const error = pdfExportError(code as PdfExportErrorCode);
    expect(error.code).toBe(code);
    expect(error.message).toBe(message);
  }
});

test("create_pdf_without_font_is_font_not_found", async () => {
  const backend = memoryBackend({ createPdf: [{ error: "ERR_PDF_EXPORT_FONT_NOT_FOUND" }] });
  const pdf = new PdfExport(backend);
  await expect(pdf.createPdf(source())).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_FONT_NOT_FOUND",
  });
});

test("create_pdf_with_bad_font_bytes_is_font_invalid", async () => {
  const backend = memoryBackend({ createPdf: [{ error: "ERR_PDF_EXPORT_FONT_INVALID" }] });
  const pdf = new PdfExport(backend);
  await expect(pdf.createPdf(source())).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_FONT_INVALID",
  });
});

test("error_message_holds_no_user_data", async () => {
  const pdf = new PdfExport(
    memoryBackend({
      convertHtml: [{ error: "ERR_PDF_EXPORT_BACKEND" }],
      savePdf: [{ error: "ERR_PDF_EXPORT_WRITE_FAILED" }],
      printPdf: [{ error: "ERR_PDF_EXPORT_PRINT_FAILED" }],
    }),
  );
  const html = "<p>SECRET-HTML-7f31</p>";
  const path = "/no/such/SECRET-PATH-8e22/report.pdf";
  const printerName = "SECRET-PRINTER-9d43";
  const errors = [
    await rejection(pdf.convertHtml(html, {})),
    await rejection(pdf.savePdf(source(), {}, path)),
    await rejection(pdf.printPdf(new Uint8Array([0x25, 0x50]), { printerName })),
  ];
  for (const error of errors) {
    expect(error.message.includes("SECRET")).toBe(false);
    expect(Object.values(MESSAGES)).toContain(error.message);
  }
});

test("a host rejection keeps its code, and an unknown one is a backend failure", () => {
  expect(asPdfExportError(pdfExportError("ERR_PDF_EXPORT_TIMEOUT")).code).toBe(
    "ERR_PDF_EXPORT_TIMEOUT",
  );
  expect(
    asPdfExportError({ code: "ERR_NSH_HOST_ERROR", message: "ERR_PDF_EXPORT_CANCELLED: text" })
      .code,
  ).toBe("ERR_PDF_EXPORT_CANCELLED");
  expect(
    asPdfExportError({ code: "ERR_NSH_HOST_ERROR", message: "ERR_PDF_EXPORT_NO_SUCH_CODE: text" })
      .code,
  ).toBe("ERR_PDF_EXPORT_BACKEND");
  expect(asPdfExportError(new Error("boom")).code).toBe("ERR_PDF_EXPORT_BACKEND");
});
