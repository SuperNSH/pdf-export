import { expect, test } from "nsh/test";
import { memoryBackend, PdfExport, type MemoryBackend, type PdfSource } from "../src";

function source(pages = 1): PdfSource {
  return {
    kind: "text",
    title: "Report",
    pages: Array.from({ length: pages }, () => ({ lines: ["a line of text"] })),
  };
}

const PDF = new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34]);

/** Real timers: the memory backend answers after `delayMs`, so tests wait them out. */
function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** The calls a memory backend took, one method name per entry. */
function methods(backend: MemoryBackend): string[] {
  return backend.calls().map((call) => call.method);
}

test("convert_html_empty_html_is_invalid_argument", async () => {
  const backend = memoryBackend();
  const pdf = new PdfExport(backend);
  await expect(pdf.convertHtml("   \t\n")).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_INVALID_ARGUMENT",
  });
  expect(methods(backend)).toEqual([]);
});

test("create_pdf_empty_pages_is_invalid_argument", async () => {
  const backend = memoryBackend();
  const pdf = new PdfExport(backend);
  await expect(pdf.createPdf({ kind: "text", title: "Empty", pages: [] })).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_INVALID_ARGUMENT",
  });
  expect(methods(backend)).toEqual([]);
});

test("custom_page_size_zero_is_invalid_argument", async () => {
  const backend = memoryBackend();
  const pdf = new PdfExport(backend);
  const sizes = [
    { widthMm: 0, heightMm: 210 },
    { widthMm: 210, heightMm: 0 },
    { widthMm: -1, heightMm: 210 },
    { widthMm: Number.NaN, heightMm: 210 },
    { widthMm: Number.POSITIVE_INFINITY, heightMm: 210 },
  ];
  for (const pageSize of sizes) {
    await expect(pdf.createPdf(source(), { pageSize })).rejects.toMatchObject({
      code: "ERR_PDF_EXPORT_INVALID_ARGUMENT",
    });
  }
  expect(methods(backend)).toEqual([]);
});

test("save_pdf_to_unwritable_path_is_write_failed", async () => {
  const backend = memoryBackend({ savePdf: [{ error: "ERR_PDF_EXPORT_WRITE_FAILED" }] });
  const pdf = new PdfExport(backend);
  await expect(pdf.savePdf(source(), {}, "/no/such/directory/doc.pdf")).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_WRITE_FAILED",
  });
});

test("save_dialog_cancel_is_cancelled", async () => {
  // A picked path of null is the user closing the dialog, so nothing is written.
  const backend = memoryBackend({ pickSavePath: [null] });
  const pdf = new PdfExport(backend);
  await expect(pdf.savePdfWithDialog(source(), {}, "report.pdf")).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_CANCELLED",
  });
  expect(backend.calls()).toContainEqual({ method: "pickSavePath", suggestedName: "report.pdf" });
  expect(methods(backend)).not.toContain("savePdf");
});

test("second_concurrent_call_is_in_progress", async () => {
  const backend = memoryBackend({ delayMs: 120 });
  const pdf = new PdfExport(backend);
  const first = pdf.createPdf(source());
  await expect(pdf.createPdf(source())).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_IN_PROGRESS",
  });
  expect(methods(backend)).toEqual(["createPdf"]);
  const firstBytes = await first;
  expect(firstBytes instanceof Uint8Array).toBe(true);
  expect(firstBytes.byteLength).toBeGreaterThan(0);
});

test("cancel_cancels_active_op_exactly_once", async () => {
  const backend = memoryBackend({ delayMs: 150 });
  const pdf = new PdfExport(backend);
  const op = pdf.createPdf(source());
  await pdf.cancel();
  await expect(op).rejects.toMatchObject({ code: "ERR_PDF_EXPORT_CANCELLED" });
  await expect(pdf.cancel()).rejects.toMatchObject({ code: "ERR_PDF_EXPORT_IN_PROGRESS" });
});

test("cancel_and_late_answer_resolves_once_cancelled", async () => {
  const backend = memoryBackend({ delayMs: 120 });
  const pdf = new PdfExport(backend);
  const op = pdf.createPdf(source());
  await pdf.cancel();
  await expect(op).rejects.toMatchObject({ code: "ERR_PDF_EXPORT_CANCELLED" });
  // The scripted answer still arrives; it must land on the settled call and disturb nothing.
  await wait(200);
  expect(methods(backend)).toEqual(["createPdf", "cancel"]);
  const next = await pdf.createPdf(source());
  expect(next.byteLength).toBeGreaterThan(0);
  expect(backend.calls().filter((call) => call.method === "createPdf")).toHaveLength(2);
});

test("bounded_wait_is_timeout_never_hangs", async () => {
  // The backend answers after 400ms; the bound gives up at 60ms, and the gate reopens.
  const backend = memoryBackend({ delayMs: 400 });
  const pdf = new PdfExport(backend).withTimeoutMs(60);
  const timedOut = pdf.createPdf(source());
  await expect(timedOut).rejects.toMatchObject({ code: "ERR_PDF_EXPORT_TIMEOUT" });
  expect(methods(backend)).toEqual(["createPdf", "cancel"]);
  await wait(500);
  expect(methods(backend)).toEqual(["createPdf", "cancel"]);
  // The gate reopened: the same instance takes the next call once its bound allows it through.
  const after = await pdf.withTimeoutMs(5_000).createPdf(source());
  expect(after.byteLength).toBeGreaterThan(0);
});

test("print_with_no_printers_is_no_printer", async () => {
  const backend = memoryBackend({
    listPrinters: [[]],
    printPdf: [{ error: "ERR_PDF_EXPORT_NO_PRINTER" }],
  });
  const pdf = new PdfExport(backend);
  expect(await pdf.listPrinters()).toEqual([]);
  await expect(pdf.printPdf(PDF, {})).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NO_PRINTER",
  });
});

test("print_to_unknown_printer_is_no_printer", async () => {
  const backend = memoryBackend({
    listPrinters: [[{ name: "Front desk", isDefault: true }]],
    printPdf: [{ error: "ERR_PDF_EXPORT_NO_PRINTER" }],
  });
  const pdf = new PdfExport(backend);
  expect(await pdf.listPrinters()).toEqual([{ name: "Front desk", isDefault: true }]);
  await expect(pdf.printPdf(PDF, { printerName: "no such printer" })).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_NO_PRINTER",
  });
});

test("in_progress_cleared_when_operation_settles", async () => {
  // The first call fails, and the very next one starts instead of answering InProgress.
  const backend = memoryBackend({ createPdf: [{ error: "ERR_PDF_EXPORT_BACKEND" }] });
  const pdf = new PdfExport(backend);
  await expect(pdf.createPdf(source())).rejects.toMatchObject({
    code: "ERR_PDF_EXPORT_BACKEND",
  });
  const next = await pdf.createPdf(source());
  expect(next.byteLength).toBeGreaterThan(0);
});
