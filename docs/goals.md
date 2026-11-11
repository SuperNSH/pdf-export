# Goals: nsh-pdf-export

What this library is for, what "done" means, and the list the tests are named against. `docs/decisions.md` records why each choice was made; `docs/parity.md` records what each API does on each platform and the run that covers it.

## Goals

1. **Create PDFs from app content.** The app supplies pages of text — v1 is text-only (`PdfSource { kind: "text", title, pages }`) — and the library renders them with `printpdf` 0.12.8, default features off, through the ops-based API (`PdfDocument` + `PdfPage` + `Op` text items). Creation is pure Rust, so `createPdf` and `saveToPath` work on every target the crate compiles for, web included. The engine needs font bytes: none provided is `FontNotFound`, bytes that will not parse are `FontInvalid`.
2. **Print where the platform allows.** iOS prints through `UIPrintInteractionController`, Windows through winspool's RAW passthrough, Linux through CUPS. macOS, Android and the web refuse with a code in v1 (`NotSupported` / `NotAvailable`), never a hang.
3. **One API, two languages.** The Rust crate `nsh_pdf_export` at `crates/nsh-pdf-export` and the TypeScript package `nsh-pdf-export` at `packages/nsh-pdf-export` expose the same names with the same behaviour: `createPdf`, `savePdf`, `savePdfWithDialog`, `convertHtml`, `printPdf`, `listPrinters`, `cancel`, `supports`. The repo is laid out like the other SuperNSH libraries, whose shape `nsh-biometrics` set: the root `Cargo.toml` is a workspace manifest whose only member is the crate, `packages/` sits beside it, docs in `docs/`, CI in `.github/workflows/`.
4. **Everything is testable without a device.** Platform work sits behind the `Backend` trait (`supports`, `create_pdf`, `save_pdf`, `pick_save_path`, `convert_html`, `print_pdf`, `list_printers`, `cancel`). `FakeBackend` (Rust, `fake.rs`) and `memoryBackend` (TS, `memory.ts`) script per-method outcomes, delays and a call log, so every rule below runs on every CI runner.

### v1 platform scope

| Platform | createPdf, saveToPath | saveDialog | print | listPrinters | convertHtml |
|---|---|---|---|---|---|
| iOS | yes | `NotSupported` | yes | `NotSupported` (v1) — `UIPrintInteractionController` offers no printer query without presenting its picker | `NotSupported` |
| macOS | yes | yes | `NotSupported` (v1) | `NotSupported` | `NotSupported` |
| Windows | yes | yes | yes | yes | `NotSupported` |
| Linux | yes | yes | yes | yes | `NotSupported` |
| Android | yes | `NotSupported` | `NotSupported` (v1) | `NotSupported` | `NotSupported` |
| Web | yes; `saveToPath` is `WriteFailed` when the host has no backing filesystem | `NotAvailable` | `NotAvailable` | `NotAvailable` | `NotAvailable` |

On a target the crate compiles for but no bridge knows, the unsupported backend answers: creation still works, and platform-only capabilities resolve `NotAvailable`. `docs/parity.md` restates this per API with the test that covers it.

## Acceptance criteria

A behaviour counts as done when all of this holds:

- The gates pass in CI on every push to `master` and on pull requests. They are host-side only; no emulator and no device is needed to run them:

  ```sh
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  cargo test --all-targets
  (cd packages/nsh-pdf-export && nsh check)
  (cd packages/nsh-pdf-export && nsh test)
  ```

- Every behaviour has a test in both languages, under the same name. The edge cases below are those names; the Rust tests live in `crates/nsh-pdf-export/tests/`, the TypeScript tests in `packages/nsh-pdf-export/tests/`. A test is seen to fail once before it counts.
- Every call resolves exactly once: a success, a coded failure, or a cancel. It never hangs and never resolves twice. One operation runs at a time per `PdfExport` instance: a second concurrent call resolves `InProgress`, the first is unaffected, and once an operation settles — any way it settles — the instance accepts the next call.
- Failures are coded errors only: one of the twelve kinds below, each with fixed wording. No panic crosses a call, and no error message holds user data — no html, no path, no printer name.
- Every operation is bounded. The default bound is 30 seconds; `PdfExport::with_timeout` (Rust) and `withTimeoutMs` (TS) replace it. On expiry the call resolves `Timeout` once and `cancel()` is attempted on the backend.
- `cancel()` with an active operation is `Ok`, and that operation resolves `Cancelled`; with no active operation it resolves `InProgress`. A late answer from the backend, arriving after a cancel or a timeout, is dropped: the caller has already resolved.
- The save dialog answering "no path" — the user cancelled it — resolves `savePdfWithDialog` as `Cancelled`.

## Errors

`PdfExportError` in Rust (`error.rs`) and TypeScript (`errors.ts`): twelve kinds, the same codes, the same messages, word for word. Rust: `ErrorKind` is `#[non_exhaustive]` and `Copy`, `PartialEq`, `Eq`, `Hash`; `Display` prints the message; `code()`, `kind()` and `status()` read it back, `status` holding a platform status where one exists. TypeScript: `PdfExportErrorCode` is the union of the twelve codes; `asPdfExportError` maps a host rejection whose message starts `"<CODE>: "` to that code, and anything unknown to `Backend`.

| Kind | Code | Message |
|---|---|---|
| `NotAvailable` | `ERR_PDF_EXPORT_NOT_AVAILABLE` | PDF export is not available on this platform. |
| `NotSupported` | `ERR_PDF_EXPORT_NOT_SUPPORTED` | This capability is not supported here yet. |
| `InvalidArgument` | `ERR_PDF_EXPORT_INVALID_ARGUMENT` | An argument is empty or not allowed on this platform. |
| `Cancelled` | `ERR_PDF_EXPORT_CANCELLED` | The operation was cancelled. |
| `Timeout` | `ERR_PDF_EXPORT_TIMEOUT` | The operation timed out. Try again. |
| `InProgress` | `ERR_PDF_EXPORT_IN_PROGRESS` | Another export or print is already running. Wait for it to finish. |
| `FontNotFound` | `ERR_PDF_EXPORT_FONT_NOT_FOUND` | The requested font is not available. |
| `FontInvalid` | `ERR_PDF_EXPORT_FONT_INVALID` | The font bytes could not be read. |
| `WriteFailed` | `ERR_PDF_EXPORT_WRITE_FAILED` | The file could not be written. |
| `NoPrinter` | `ERR_PDF_EXPORT_NO_PRINTER` | No printer is available. |
| `PrintFailed` | `ERR_PDF_EXPORT_PRINT_FAILED` | The print job failed. |
| `Backend` | `ERR_PDF_EXPORT_BACKEND` | The platform's PDF export service failed. |

The native module (`nsh-pdf-export/native`) rejects with a message of the form `"<CODE>: <message>"`; `rust.ts` settles every rejection through `asPdfExportError`, so both languages see the same code.

## Edge cases

Twenty-two named cases, grouped. The name is the test name, identical in both languages; the Rust test is in `crates/nsh-pdf-export/tests/`, the TypeScript test in `packages/nsh-pdf-export/tests/`. The number is the case's number in the contract.

### Arguments

| # | Test | Checks |
|---|---|---|
| 1 | `convert_html_empty_html_is_invalid_argument` | Empty or whitespace-only html resolves `InvalidArgument`. |
| 2 | `create_pdf_empty_pages_is_invalid_argument` | A source with no pages resolves `InvalidArgument`. |
| 4 | `custom_page_size_zero_is_invalid_argument` | A custom page size with zero, negative or non-finite mm resolves `InvalidArgument`. |
| 20 | `page_size_system_default_respects_locale` | `systemDefault("en-US")` is `letter`, any other locale `a4`; `sizeMm` returns the fixed table (A4 210x297, A3 297x420, A5 148x210, Letter 215.9x279.4, Legal 215.9x355.6, Tabloid 279.4x431.8). |

`PrintOptions.copies` outside 1..=999 resolves `InvalidArgument` under the same rule; it has no named case of its own.

### Fonts

| # | Test | Checks |
|---|---|---|
| 5 | `create_pdf_without_font_is_font_not_found` | Creating a PDF with no font bytes resolves `FontNotFound`. |
| 6 | `create_pdf_with_bad_font_bytes_is_font_invalid` | Font bytes that will not parse resolve `FontInvalid`. |

### Filesystem

| # | Test | Checks |
|---|---|---|
| 7 | `save_pdf_to_unwritable_path_is_write_failed` | Saving to a path the process cannot write resolves `WriteFailed`. |
| 8 | `save_dialog_cancel_is_cancelled` | The save dialog answered with no path — the user cancelled — resolves `Cancelled`. |

### Lifecycle

| # | Test | Checks |
|---|---|---|
| 9 | `second_concurrent_call_is_in_progress` | A second call while one runs resolves `InProgress`; the first is unaffected. |
| 10 | `cancel_cancels_active_op_exactly_once` | `cancel()` with an active operation is `Ok` and the operation resolves `Cancelled` exactly once; a second `cancel` resolves `InProgress`. |
| 11 | `cancel_and_late_answer_resolves_once_cancelled` | A scripted backend answer arriving after the cancel is dropped; the caller saw `Cancelled`, exactly once. |
| 12 | `bounded_wait_is_timeout_never_hangs` | A backend slower than the bound resolves `Timeout` exactly once — the fake still answers once afterwards — and the instance takes the next call. |
| 21 | `in_progress_cleared_when_operation_settles` | After an operation settles, the next call starts instead of resolving `InProgress`. |
| 22 | `error_message_holds_no_user_data` | No error message contains the html, the path or the printer name the call was given. |

### Platform matrix

| # | Test | Checks |
|---|---|---|
| 13 | `print_with_no_printers_is_no_printer` | Printing with no printer available resolves `NoPrinter`. |
| 14 | `print_to_unknown_printer_is_no_printer` | A `printerName` matching no installed printer resolves `NoPrinter`. |
| 15 | `web_print_is_not_available` | The web backend resolves print and `listPrinters` as `NotAvailable`. |
| 16 | `android_print_is_not_supported` | The Android bridge resolves print as `NotSupported` in v1. |
| 17 | `macos_print_is_not_supported_in_v1` | The macOS bridge resolves print as `NotSupported` in v1. |
| 18 | `convert_html_is_not_supported_in_v1` | `convertHtml` resolves `NotSupported` on every native platform in v1, `NotAvailable` on the web. |

### Engine

| # | Test | Checks |
|---|---|---|
| 3 | `create_pdf_blank_page_is_valid_pdf` | A page with no lines still produces bytes starting `%PDF-`. |
| 19 | `create_pdf_large_document_resolves` | 500 pages resolve with a `%PDF-` header and no hang. |

The engine cases render with the test font vendored at `crates/nsh-pdf-export/assets/export-test-font.ttf` (copied from the printpdf sources in `.refs/printpdf`) and assert the output starts `%PDF-`.
