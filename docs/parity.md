# Parity

How our API lines up with the references cloned into `.refs/`. `docs/decisions.md` owns the
v1 platform matrix; this file restates it per API and records every difference.

Honesty rule, unchanged from the stub: a row is **not started** until CI runs a test for it
on `master`. **Out of v1** marks a deliberate gap recorded here, not a work item. No row is
done today; the crate and package are being built against this table.

"Ours" is the identically named API in `crates/nsh-pdf-export` (Rust) and
`packages/nsh-pdf-export` (TS). Errors are cited by kind name; codes and wording live in
`error.rs` / `errors.ts`.

## printing (Dart, DavBfr/dart_pdf)

Reference: `.refs/dart_pdf/printing/lib/src/printing.dart` (the `Printing` mixin),
`printer.dart`, `printing_info.dart`, `output_type.dart`, and `PdfPageFormat` from
`.refs/dart_pdf/pdf/lib/src/pdf/page_format.dart`.

| printing API | Ours in v1 | Status |
|---|---|---|
| `Printing.layoutPdf({onLayout, name, format, dynamicLayout, usePrinterSettings, outputType, forceCustomPrintPaper, windowsModernDialog})` — print through the platform UI, rebuilding the document when settings change | `printPdf(pdf, options?)` prints already-built bytes: iOS shows `UIPrintInteractionController`, Windows (winspool RAW) and Linux (libcups) print silently. macOS and Android reject `NotSupported`; web rejects `NotAvailable`. There is no layout callback, so `dynamicLayout`, `usePrinterSettings`, `forceCustomPrintPaper` and `windowsModernDialog` have no equivalent; `name` maps to `PrintOptions.jobName`; page size is fixed at `createPdf` time, not re-chosen in the dialog. | not started |
| `Printing.listPrinters()` | `listPrinters()` returning `Printer[]`; supported on Windows and Linux. iOS rejects `NotSupported` in v1 — `UIPrintInteractionController` offers no printer query without its picker — as do macOS and Android; web rejects `NotAvailable`. | not started |
| `Printing.pickPrinter({context, bounds, title})` | No equivalent. On Windows and Linux callers list with `listPrinters()` and pass `PrintOptions.printerName` to `printPdf` — the same fallback the Dart method uses on platforms that can list printers. On iOS that fallback cannot work: `listPrinters` rejects `NotSupported` and `printPdf` refuses a named printer with `InvalidArgument` — the sheet's own picker chooses. | out of v1 |
| `Printing.directPrintPdf({printer, onLayout, name, format, ...})` — silent print to a chosen printer | `printPdf(pdf, { printerName, jobName, copies })`. `copies` is 1..=999, default 1; out of range rejects `InvalidArgument`. Printers are addressed by display name; `printer.url` has no counterpart. | not started |
| `Printing.sharePdf({bytes, filename, bounds, subject, body, emails})` | No equivalent. Closest is `savePdfWithDialog` on desktop. | out of v1 |
| `Printing.convertHtml({html, baseUrl, format})` — deprecated upstream | `convertHtml(html, options?)` with `HtmlOptions { paddingMm, backgroundColor, baseUrl }`. No engine is wired in v1: every native platform rejects `NotSupported`, web rejects `NotAvailable`. Recorded here so the gap stays visible. | not started |
| `Printing.info()` — capability snapshot object | `supports(capability)` returning a boolean, one call per capability; no snapshot struct to go stale. | not started |
| `Printing.raster(document, {pages, dpi})` — PDF pages to images, default dpi `PdfPageFormat.inch` (72) | No equivalent. | out of v1 |
| `Printer { url, name, model, location, comment, isDefault, isAvailable }` | `Printer { name, isDefault }`. The other fields are dropped. | not started |
| `PrintingInfo { directPrint, dynamicLayout, canPrint, canConvertHtml, canListPrinters, canShare, canRaster }` | `supports(capability)` over `"createPdf" \| "saveToPath" \| "saveDialog" \| "convertHtml" \| "print" \| "listPrinters"`. `directPrint` and `canPrint` fold into `"print"`; `dynamicLayout`, `canShare` and `canRaster` have no capability because v1 has no matching feature. | not started |
| `OutputType { generic, photo, grayscale, photoGrayscale }` (iOS print systems) | Not modeled; iOS prints generic only. | out of v1 |
| `PdfPageFormat`: `a3`, `a4`, `a5`, `a6`, `letter`, `legal`, `roll57`, `roll80`, `undefined`, `standard = a4`; per-side margins; point/inch/cm/mm/dp units | `PageSize = "a3" \| "a4" \| "a5" \| "letter" \| "legal" \| "tabloid" \| { widthMm, heightMm }` with `sizeMm(PageSize)` and `systemDefault(locale)` (`"en-US"` → letter, anything else → a4). Names we share are the same physical size (a3 297×420, a4 210×297, a5 148×210, letter 215.9×279.4, legal 215.9×355.6 mm). Dropped: `a6`, `roll57`, `roll80`, `undefined`, per-side margins (v1 text pages lay out their own spacing; HTML padding is `HtmlOptions.paddingMm`). Added: `tabloid` 279.4×431.8. With `pageSize` unset the engine draws a fixed `a4`; `systemDefault(locale)` (`system_default` in the crate) is a pure helper callers may use to pick a locale-appropriate size — the reference's locale-based default is not automatic in ours. | not started |

## react-native-html-to-pdf

Reference: `.refs/react-native-html-to-pdf/src/NativeHtmlToPdf.ts`.

| Reference API | Ours in v1 | Status |
|---|---|---|
| `Spec.convert(options: PDFOptions): Promise<PDFResult>` | `convertHtml(html, options?)` returning PDF bytes. No engine in v1: native rejects `NotSupported`, web `NotAvailable`. | not started |
| `PDFOptions.html` | First argument of `convertHtml`. | not started |
| `PDFOptions.fileName` | No direct equivalent; the output name comes from `savePdf(source, options, path)` or `savePdfWithDialog(source, options, suggestedName)`. The reference does not always write a temp file: iOS writes into Documents when `PDFOptions.directory` is `"Documents"`, else `NSTemporaryDirectory()`; Android writes a cache-dir temp file only when `directory` is unset — with it set, the file goes to `getExternalFilesDir(null)/<directory>` (or `filesDir/<directory>` when external storage is unmounted). Ours never writes unless asked. | not started |
| `PDFOptions.width` / `height` (points) | `ExportOptions.pageSize`, in mm not points, fixed at `createPdf`. `HtmlOptions` has no width/height in v1. | not started |
| `PDFOptions.base64` | No equivalent; `createPdf` hands back bytes. | out of v1 |
| `PDFOptions.padding`, `paddingTop`, `paddingBottom`, `paddingLeft`, `paddingRight` | `HtmlOptions.paddingMm`, one value applied to all four sides. | not started |
| `PDFOptions.bgColor` | `HtmlOptions.backgroundColor`. | not started |
| `PDFOptions.directory` | No equivalent; `savePdf` takes a full path. | out of v1 |
| `PDFOptions.baseURL` | `HtmlOptions.baseUrl`. | not started |
| `PDFOptions.shouldPrintBackgrounds` | No equivalent; background rendering follows `backgroundColor` once an engine exists. | out of v1 |
| `PDFResult.filePath` | Not returned by `convertHtml` (which rejects in v1 anyway); file writes go through `savePdf` / `savePdfWithDialog`. | not started |
| `PDFResult.base64` | No equivalent. | out of v1 |
| `PDFResult.numberOfPages` | No equivalent in the HTML path; the text path's page count is `PdfSource.pages.length` at `createPdf`. | out of v1 |

## printpdf (the creation engine we embed)

`printpdf 0.12.8` with default features off (decisions.md, item 3): the ops-based API needs
none of the `html` feature's layout engine or fontconfig. `build_pdf` maps onto it:

| printpdf | `build_pdf` |
|---|---|
| `PdfDocument::new(name)` — `src/lib.rs` | One document per `PdfSource`; the title comes from `PdfSource.title` / `ExportOptions.title`. |
| `PdfPage::new(Mm, Mm, ops)` — `src/ops.rs` | One page per `PageText`; the media box comes from `sizeMm(ExportOptions.pageSize)`. |
| `Op` — `src/ops.rs` | Per page: `StartTextSection`, `SetFont { font, size: Pt }`, `EndTextSection`; per line: `SetTextCursor`, `ShowText { items: [TextItem::Text(line)] }` (`src/text.rs`). No `AddLineBreak` is emitted. Every backend embeds the same vendored external TTF (Roboto Medium, copied from printpdf's example fonts) and registers it with `add_font` → `PdfFontHandle::External`. `FontNotFound` covers a call with no font — the front API supplies none, so the bridges own the embed; `FontInvalid` covers bytes no parser reads. |
| `PdfDocument::save(&PdfSaveOptions, &mut warnings)` — `src/lib.rs`, options in `src/serialize.rs` | Serializes the final bytes with default options (optimize, subset fonts, secure). |

Status: not started.

## Our front API against the v1 matrix

One export or print runs at a time; a second call while one is in flight rejects
`InProgress`. An operation ends by succeeding, by `cancel()` (resolving
`Cancelled`), by a save dialog the user dismisses (also `Cancelled`), or when the
per-instance bound expires (`Timeout`) — never a hang.

| Method | macOS | Windows | Linux | iOS | Android | web | Status |
|---|---|---|---|---|---|---|---|
| `createPdf(source, options?)` | yes | yes | yes | yes | yes | yes | not started |
| `savePdf(source, options?, path)` | yes | yes | yes | yes | yes | `WriteFailed` when the host has no backing filesystem | not started |
| `savePdfWithDialog(source, options?, suggestedName)` | yes | yes | yes | `NotSupported` | `NotSupported` | `NotAvailable` | not started |
| `convertHtml(html, options?)` | `NotSupported` | `NotSupported` | `NotSupported` | `NotSupported` | `NotSupported` | `NotAvailable` | not started |
| `printPdf(pdf, options?)` | `NotSupported` in v1 | yes — winspool `WritePrinter` RAW passthrough | yes — libcups FFI, `#[link(name = "cups")]` | yes — `UIPrintInteractionController` via objc2-uikit | `NotSupported` — `PrintManager` needs a Java `PrintDocumentAdapter` an NSH app cannot ship | `NotAvailable` | not started |
| `listPrinters()` | `NotSupported` | yes | yes | `NotSupported` — `UIPrintInteractionController` offers no printer query without its picker | `NotSupported` | `NotAvailable` | not started |
| `cancel()` | yes | yes | yes | yes | yes | yes | not started |
| `supports(capability)` | yes | yes | yes | yes | yes | yes | not started |
