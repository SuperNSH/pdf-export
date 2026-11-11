import { expect, test } from "nsh/test";
import {
  memoryBackend,
  PdfExport,
  sizeMm,
  systemDefault,
  type PdfSource,
} from "../src";

/** The header every PDF carries; the memory backend hands out exactly these bytes. */
const PDF_HEADER = "%PDF-";

function text(bytes: Uint8Array): string {
  return String.fromCharCode(...bytes.subarray(0, PDF_HEADER.length));
}

function source(pages: number, lines = 1): PdfSource {
  return {
    kind: "text",
    title: "Report",
    pages: Array.from({ length: pages }, () => ({
      lines: Array.from({ length: lines }, (_, index) => `line ${index}`),
    })),
  };
}

test("create_pdf_blank_page_is_valid_pdf", async () => {
  const pdf = new PdfExport(memoryBackend());
  const bytes = await pdf.createPdf({ kind: "text", title: "Blank", pages: [{ lines: [] }] });
  expect(text(bytes)).toBe(PDF_HEADER);
});

test("create_pdf_large_document_resolves", async () => {
  const pdf = new PdfExport(memoryBackend());
  const bytes = await pdf.createPdf(source(500, 40));
  expect(text(bytes)).toBe(PDF_HEADER);
});

test("web_print_is_not_available", async () => {
  // The web matrix: creation only, and every platform-only capability answers NotAvailable.
  const backend = memoryBackend({
    capabilities: ["createPdf", "saveToPath"],
    printPdf: [{ error: "ERR_PDF_EXPORT_NOT_AVAILABLE" }],
    listPrinters: [{ error: "ERR_PDF_EXPORT_NOT_AVAILABLE" }],
  });
  const pdf = new PdfExport(backend);
  expect(pdf.supports("createPdf")).toBe(true);
  expect(pdf.supports("print")).toBe(false);
  expect(pdf.supports("listPrinters")).toBe(false);
  await expect(pdf.printPdf(new Uint8Array([0x25, 0x50]), {})).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NOT_AVAILABLE",
  });
  await expect(pdf.listPrinters()).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NOT_AVAILABLE",
  });
});

test("android_print_is_not_supported", async () => {
  // Android v1: PrintManager needs a Java adapter an NSH app cannot ship, so printing refuses.
  const backend = memoryBackend({
    capabilities: ["createPdf", "saveToPath"],
    printPdf: [{ error: "ERR_PDF_EXPORT_NOT_SUPPORTED" }],
  });
  const pdf = new PdfExport(backend);
  expect(pdf.supports("print")).toBe(false);
  await expect(pdf.printPdf(new Uint8Array([0x25, 0x50]), {})).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NOT_SUPPORTED",
  });
});

test("macos_print_is_not_supported_in_v1", async () => {
  // The macOS matrix: creation and the save dialog, but printing is not wired in v1.
  const backend = memoryBackend({
    capabilities: ["createPdf", "saveToPath", "saveDialog"],
    printPdf: [{ error: "ERR_PDF_EXPORT_NOT_SUPPORTED" }],
  });
  const pdf = new PdfExport(backend);
  expect(pdf.supports("print")).toBe(false);
  await expect(pdf.printPdf(new Uint8Array([0x25, 0x50]), {})).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NOT_SUPPORTED",
  });
});

test("convert_html_is_not_supported_in_v1", async () => {
  // Native platforms have no engine wired in v1; the web has none behind it at all.
  const native = new PdfExport(
    memoryBackend({ convertHtml: [{ error: "ERR_PDF_EXPORT_NOT_SUPPORTED" }] }),
  );
  await expect(native.convertHtml("<p>Hello</p>", {})).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NOT_SUPPORTED",
  });
  const web = new PdfExport(
    memoryBackend({
      capabilities: ["createPdf", "saveToPath"],
      convertHtml: [{ error: "ERR_PDF_EXPORT_NOT_AVAILABLE" }],
    }),
  );
  await expect(web.convertHtml("<p>Hello</p>", {})).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NOT_AVAILABLE",
  });
});

test("page_size_system_default_respects_locale", () => {
  expect(systemDefault("en-US")).toBe("letter");
  expect(systemDefault("en_US")).toBe("a4");
  expect(systemDefault("fr-FR")).toBe("a4");
  expect(systemDefault("")).toBe("a4");
  expect(sizeMm(systemDefault("en-US"))).toEqual({ widthMm: 215.9, heightMm: 279.4 });
  expect(sizeMm(systemDefault("de-DE"))).toEqual({ widthMm: 210, heightMm: 297 });
  expect(sizeMm("a3")).toEqual({ widthMm: 297, heightMm: 420 });
  expect(sizeMm("a5")).toEqual({ widthMm: 148, heightMm: 210 });
  expect(sizeMm("letter")).toEqual({ widthMm: 215.9, heightMm: 279.4 });
  expect(sizeMm("legal")).toEqual({ widthMm: 215.9, heightMm: 355.6 });
  expect(sizeMm("tabloid")).toEqual({ widthMm: 279.4, heightMm: 431.8 });
  expect(sizeMm({ widthMm: 100, heightMm: 200 })).toEqual({ widthMm: 100, heightMm: 200 });
});
