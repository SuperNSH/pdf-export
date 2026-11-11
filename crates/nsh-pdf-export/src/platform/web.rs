//! The web (wasm32) backend. PDF bytes are built in memory by the crate's own engine; the
//! browser offers no print spooler, HTML engine or save dialog in v1, so those answer
//! `ERR_PDF_EXPORT_NOT_AVAILABLE`, and a save resolves `WriteFailed` where no host fs backs
//! the target.

use std::path::{Path, PathBuf};

use crate::backend::{Backend, BackendFuture, Capability, PrintJob, Printer};
use crate::error::{ErrorKind, PdfExportError};
use crate::export::build_pdf;
use crate::options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions};

// printpdf needs embedded font bytes and the public API carries none, so every bridge
// embeds the crate's vendored font (Roboto Medium, .refs/printpdf/examples/assets/fonts).
const FONT: &[u8] = include_bytes!("../../assets/export-test-font.ttf");

/// The web backend. It holds no state; construct it on any target to exercise the
/// not-available paths in host tests.
#[derive(Default)]
pub struct WebBackend;

impl WebBackend {
    /// A backend ready to answer; every answer follows from the platform matrix.
    pub fn new() -> Self {
        Self
    }
}

/// The web answer to every platform-only capability, resolved at once.
fn not_available<T>() -> BackendFuture<'static, Result<T, PdfExportError>> {
    Box::pin(async { Err(PdfExportError::new(ErrorKind::NotAvailable)) })
}

impl Backend for WebBackend {
    fn supports(&self, capability: Capability) -> bool {
        // Creation is pure computation and a save is attempted wherever a host fs exists;
        // everything else in the matrix is desktop- or spooler-bound.
        matches!(capability, Capability::CreatePdf | Capability::SaveToPath)
    }

    // The trait's future borrows only `&self`, so the arguments cannot live in it; each is
    // cloned in. One copy per export is nothing next to encoding the pages.
    fn create_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        let source = source.clone();
        let options = options.clone();
        Box::pin(async move { build_pdf(&source, &options, Some(FONT)).await })
    }

    fn save_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
        path: &Path,
    ) -> BackendFuture<'_, Result<PathBuf, PdfExportError>> {
        let source = source.clone();
        let options = options.clone();
        let path = path.to_path_buf();
        Box::pin(async move {
            let pdf = build_pdf(&source, &options, Some(FONT)).await?;
            // The shared writer errs on wasm32-unknown-unknown (no backing fs), which
            // resolves WriteFailed — the coded refusal, never a panic.
            super::write_pdf_to_path(&pdf, &path).await
        })
    }

    /// No browser save dialog reaches a path an NSH app could return, so v1 refuses.
    fn pick_save_path(
        &self,
        _suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
        not_available()
    }

    /// No HTML engine is wired into the web bridge in v1.
    fn convert_html(
        &self,
        _html: &str,
        _options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        not_available()
    }

    /// No print spooler exists behind a browser tab.
    fn print_pdf(
        &self,
        _pdf: &[u8],
        _options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
        not_available()
    }

    /// Printers follow printing, which is refused, so there is nothing to list.
    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
        not_available()
    }

    /// Creation is pure computation that settles on its own, so a cancel always resolves.
    fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>> {
        Box::pin(async { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    use super::*;

    /// Runs `future` to its single answer on the test thread; these futures never yield `Pending`.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn one_page() -> PdfSource {
        PdfSource::text(
            "Report",
            vec!["first line".to_owned(), "second line".to_owned()],
        )
    }

    #[test]
    fn supports_reports_the_web_matrix() {
        let backend = WebBackend::new();
        assert!(backend.supports(Capability::CreatePdf));
        assert!(backend.supports(Capability::SaveToPath));
        for unavailable in [
            Capability::SaveDialog,
            Capability::ConvertHtml,
            Capability::Print,
            Capability::ListPrinters,
        ] {
            assert!(!backend.supports(unavailable));
        }
    }

    #[test]
    fn print_pdf_is_not_available() {
        let Err(error) =
            block_on(WebBackend::new().print_pdf(b"%PDF".as_slice(), &PrintOptions::default()))
        else {
            panic!("printing must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotAvailable));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_AVAILABLE");
    }

    #[test]
    fn list_printers_is_not_available() {
        let Err(error) = block_on(WebBackend::new().list_printers()) else {
            panic!("listing must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotAvailable));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_AVAILABLE");
    }

    #[test]
    fn pick_save_path_is_not_available() {
        let Err(error) = block_on(WebBackend::new().pick_save_path("report.pdf")) else {
            panic!("the picker must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotAvailable));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_AVAILABLE");
    }

    #[test]
    fn convert_html_is_not_available() {
        let Err(error) =
            block_on(WebBackend::new().convert_html("<p>Report</p>", &HtmlOptions::default()))
        else {
            panic!("conversion must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotAvailable));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_AVAILABLE");
    }

    #[tokio::test]
    async fn create_pdf_builds_bytes_with_the_crate_font() {
        let bytes = WebBackend::new()
            .create_pdf(&one_page(), &ExportOptions::default())
            .await
            .expect("creation must build");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[tokio::test]
    async fn save_pdf_to_a_file_as_parent_is_write_failed() {
        let root = std::env::temp_dir().join("nsh-pdf-export-web-save");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir must create");
        let blocker = root.join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("blocker must create");
        let target = blocker.join("doc.pdf");
        let error = WebBackend::new()
            .save_pdf(&one_page(), &ExportOptions::default(), &target)
            .await
            .expect_err("the save must refuse under a file parent");
        assert!(matches!(error.kind(), ErrorKind::WriteFailed));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_WRITE_FAILED");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_resolves_with_nothing_running() {
        let Ok(()) = block_on(WebBackend::new().cancel()) else {
            panic!("cancel must resolve");
        };
    }
}
