# Decisions

## Open

None.

## Decided

1. **Reference URLs.** All five canonical URLs from the brief cloned into `.refs/` on 2026-11-09, no redirects, no corrections:
   `DavBfr/dart_pdf` (the `printing` package lives at the repo root, not `packages/printing`),
   `christopherdro/react-native-html-to-pdf`, `fschutt/printpdf`, `longbridge/nsh`, `SuperNSH/biometrics`.
2. **NSH version.** NSH 0.7.1 (moved from 0.4.0 to 0.5.0 on 2026-11-12, to 0.5.1 on 2026-11-17, to 0.6.0 on 2026-11-18, and to 0.7.0 on 2026-11-25). The CLI CI installs (`NSH_VERSION: v0.7.1`) and `nsh-core = "0.7.0"` (resolves to 0.7.1) match.
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
5. **Dependency licenses.** Verified from registry metadata (2026-11-17, `cargo metadata`)
   before pinning, closing the brief's research note. Every direct dependency is permissive —
   MIT, Apache-2.0, or an either/or of those with Zlib — so the crate's MIT license holds:
   nsh-core 0.7.1 Apache-2.0; printpdf MIT (requirement `0.12.8`, lock resolves 0.12.9);
   thiserror, base64, serde_json (dev), jni, ndk-context, libc, windows MIT OR Apache-2.0;
   tokio, objc2, objc2-foundation, block2 MIT; objc2-ui-kit and dispatch2 Zlib OR Apache-2.0 OR MIT.
6. **App fonts and streamed saves.** `ExportOptions.font` carries an app's own TTF/OTF bytes
   through the public API (the crate's vendored Roboto Medium when unset; unparseable bytes
   answer `ERR_PDF_EXPORT_FONT_INVALID`), and `save_pdf` streams printpdf's serializer
   straight into the file writer, so the serialized PDF is never held whole on top of the
   document model. The model itself is still laid out in memory — page-by-page streaming
   needs an engine lopdf does not offer; that is the honest boundary of "stream pages" here.

