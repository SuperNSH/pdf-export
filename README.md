# nsh-pdf-export

Creating PDFs from app content, and printing where the platform allows, for [Native Shell](https://nsh.dev) apps — in Rust and in TSX, with the same names.

What it does:

- Builds a real PDF from the app's own content. v1 takes pages of text; [`printpdf`](https://crates.io/crates/printpdf) renders them, so creation works on every target the crate compiles for, web included.
- Saves the PDF to a path the app names, or through the desktop save dialog.
- Prints where the platform allows — iOS, Windows, Linux — and refuses with a stable code where it does not.
- Runs one operation at a time per instance. Every call resolves exactly once: a result, a coded failure, or a cancel. Nothing hangs.

[docs/goals.md](docs/goals.md) states the goals, the acceptance criteria, the twelve error kinds and the twenty-two named edge cases. [docs/decisions.md](docs/decisions.md) says why each choice was made, and [docs/parity.md](docs/parity.md) says what each API does on each platform and names the test that covers it.

## Layout

The same shape as the other SuperNSH libraries:

- `crates/nsh-pdf-export/` — the Rust crate `nsh_pdf_export`: the error kinds, the types and options, the printpdf engine, the `Backend` trait with one bridge per platform and `FakeBackend`, and the `nsh-pdf-export/native` module for TSX.
- `packages/nsh-pdf-export/` — the TypeScript package `nsh-pdf-export`: the same API over the native module, with `memoryBackend` for tests.
- `docs/` — `brief.md` (the research brief), `goals.md`, `decisions.md`, `parity.md`.
- `.github/workflows/ci.yml` — the gates CI runs on every push to `master` and on pull requests; `pages.yml` publishes `site/`, the status page, to [supernsh.github.io/pdf-export](https://supernsh.github.io/pdf-export/).

The root `Cargo.toml` is the workspace manifest; `crates/nsh-pdf-export` is its only member.

## Use it in Rust

Add the crate to `Cargo.toml`:

```toml
[dependencies]
nsh-pdf-export = { git = "https://github.com/SuperNSH/pdf-export" }
```

```rust
use nsh_pdf_export::{ExportOptions, PageText, PdfExport, PdfSource};

let source = PdfSource {
    kind: "text",
    title: "Notes".into(),
    pages: vec![PageText { lines: vec!["Hello PDF".into()] }],
};
let export = PdfExport::with_default_backend();
let bytes = export.create_pdf(&source, &ExportOptions::default()).await?;
export
    .save_pdf(&source, &ExportOptions::default(), "notes.pdf".as_ref())
    .await?;
```

Match `error.kind()` for the cases the app handles, such as `Cancelled` or `NoPrinter`; `error.code()` gives the stable string and `error.status()` the platform status where one exists.

## Use it in TSX

List the crate under `"rust"` in the app's `nsh.json`, and add the TypeScript package from `packages/nsh-pdf-export`:

```tsx
import { mount, Text, View } from "nsh";
import { PdfExport, PdfExportError } from "nsh-pdf-export";

const pdfExport = new PdfExport();

function App() {
  const saveNotes = () => {
    const source = { kind: "text", title: "Notes", pages: [{ lines: ["Hello PDF"] }] };
    pdfExport.savePdf(source, undefined, "notes.pdf").then(
      (path) => console.log(path),
      (error: unknown) =>
        console.log(error instanceof PdfExportError ? error.code : "unknown error"),
    );
  };
  return (
    <View class="flex-1 p-4">
      <Text role="button" onPress={saveNotes} class="rounded-(--radius) bg-muted px-3 py-2">Save PDF</Text>
    </View>
  );
}

mount(App);
```

Every rejection settles as a `PdfExportError` with a `code` such as `ERR_PDF_EXPORT_CANCELLED` and the same message the Rust side prints; `asPdfExportError(error)` maps anything a host throws onto the same twelve codes.

## Platforms

What each capability does on each platform in v1:

| Platform | createPdf, saveToPath | saveDialog | print | listPrinters | convertHtml |
|---|---|---|---|---|---|
| iOS | yes | `ERR_PDF_EXPORT_NOT_SUPPORTED` | yes (`UIPrintInteractionController`) | `ERR_PDF_EXPORT_NOT_SUPPORTED` in v1 — no printer query without presenting the picker | `ERR_PDF_EXPORT_NOT_SUPPORTED` |
| macOS | yes | yes | `ERR_PDF_EXPORT_NOT_SUPPORTED` in v1 | `ERR_PDF_EXPORT_NOT_SUPPORTED` | `ERR_PDF_EXPORT_NOT_SUPPORTED` |
| Windows | yes | yes | yes (winspool RAW passthrough) | yes | `ERR_PDF_EXPORT_NOT_SUPPORTED` |
| Linux | yes | yes | yes (CUPS) | yes | `ERR_PDF_EXPORT_NOT_SUPPORTED` |
| Android | yes | `ERR_PDF_EXPORT_NOT_SUPPORTED` | `ERR_PDF_EXPORT_NOT_SUPPORTED` in v1 | `ERR_PDF_EXPORT_NOT_SUPPORTED` | `ERR_PDF_EXPORT_NOT_SUPPORTED` |
| Web | yes; `saveToPath` is `ERR_PDF_EXPORT_WRITE_FAILED` when the host has no backing filesystem | `ERR_PDF_EXPORT_NOT_AVAILABLE` | `ERR_PDF_EXPORT_NOT_AVAILABLE` | `ERR_PDF_EXPORT_NOT_AVAILABLE` | `ERR_PDF_EXPORT_NOT_AVAILABLE` |

Printing takes PDF bytes and a `PrintOptions` (`printerName`, `jobName`, `copies` 1..=999); `listPrinters()` works on Windows and Linux only. On a target no bridge knows, creation still works and platform-only capabilities resolve `ERR_PDF_EXPORT_NOT_AVAILABLE`.

Limits:

- **v1 is text-only.** `PdfSource` carries `kind: "text"`. `convertHtml` resolves `ERR_PDF_EXPORT_NOT_SUPPORTED` on every native platform — no engine is wired in v1; `docs/parity.md` records it — and `ERR_PDF_EXPORT_NOT_AVAILABLE` on the web.
- **macOS and Android do not print in v1.** Android's `PrintManager` needs a Java `PrintDocumentAdapter` an NSH app cannot ship; macOS printing waits for a reason to wire it.
- **Creation needs a font.** A backend with no font bytes resolves `ERR_PDF_EXPORT_FONT_NOT_FOUND`; bytes that will not parse resolve `ERR_PDF_EXPORT_FONT_INVALID`.
- **One operation at a time per instance.** A second concurrent call resolves `ERR_PDF_EXPORT_IN_PROGRESS` and the first is unaffected. A second instance runs a second export concurrently.
- **Every operation is bounded.** 30 seconds by default; `withTimeoutMs` (Rust: `with_timeout`) replaces it. Expiry resolves `ERR_PDF_EXPORT_TIMEOUT`, and `cancel()` is attempted on the backend.
- **No error message holds user data.** Not the html, not the path, not the printer name.

## What CI verifies

CI runs on every push to `master` and on pull requests, on GitHub's runners:

- **Format**: `cargo fmt --all -- --check`.
- **Rust suite**: `cargo clippy --all-targets -- -D warnings`, then `cargo test --all-targets`. Through `FakeBackend` the suite covers the front API's rules — one operation at a time, resolve-once, cancel, the bound — and the engine renders real output asserted to start `%PDF-`, including a 500-page document.
- **TypeScript suite**: `nsh check`, then `nsh test` in `packages/nsh-pdf-export`, over `memoryBackend`.

The platform bridges are compile-checked in CI for their targets. **Native printing is compile-checked in CI only, never device-tested.** No runner has a printer, an iOS device or an Android device attached; no print job has been sent from this code, and no save or print dialog has been shown on a runner. `docs/parity.md` names, per capability, the test that covers it; a row there with no passing run is unverified.

## Development

Nothing builds on a developer's machine. GitHub Actions compiles and tests everything; commit, push to `master`, and read the results with `gh run`. `cargo fmt` is the only local command the repo expects.

## License

MIT, as in [LICENSE](LICENSE).

Repository: https://github.com/SuperNSH/pdf-export
