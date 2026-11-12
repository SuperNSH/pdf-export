# Decisions

## Open

None.

## Decided

1. **Reference URLs.** All five canonical URLs from the brief cloned into `.refs/` on 2026-11-09, no redirects, no corrections:
   `DavBfr/dart_pdf` (the `printing` package lives at the repo root, not `packages/printing`),
   `christopherdro/react-native-html-to-pdf`, `fschutt/printpdf`, `longbridge/nsh`, `SuperNSH/biometrics`.
2. **NSH version.** NSH 0.5.0 (moved from 0.4.0 on 2026-11-12). The CLI CI installs (`NSH_VERSION: v0.5.0`) and `nsh-core = "0.5.0"` match.
3. **Library choice.** `printpdf = { version = "0.12.8", default-features = false }` for PDF creation.
   The default `html` feature pulls the azul layout engine and fontconfig; the ops-based API
   (`PdfDocument` + `PdfPage::new` + `Op` text items + `save`) needs none of that. `convert_html`
   maps to platform engines where one exists and to a coded `ERR_PDF_EXPORT_NOT_SUPPORTED` elsewhere.
4. **Platforms in scope.** Native printing: iOS (`UIPrintInteractionController` via objc2-uikit),
   Windows (winspool RAW passthrough), Linux (libcups FFI). Android's `PrintManager` needs a
   `PrintDocumentAdapter`, which only exists as a Java class an NSH app cannot ship, so the
   Android bridge reports coded `ERR_PDF_EXPORT_NOT_SUPPORTED` in v1; macOS printing and the
   web report coded `ERR_PDF_EXPORT_NOT_SUPPORTED` / `ERR_PDF_EXPORT_NOT_AVAILABLE`. PDF
   creation and saving to an explicit path work everywhere the crate compiles. Recorded in
   `docs/parity.md` per API.
