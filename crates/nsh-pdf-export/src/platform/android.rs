//! The Android bridge: creation runs the crate's own engine, so it works wherever the crate
//! compiles. Capabilities that need Java — printing, the storage picker — answer
//! `ERR_PDF_EXPORT_NOT_SUPPORTED` in v1; the jni/ndk-context/libc dependencies wait for them.

use std::path::{Path, PathBuf};

use crate::{
    backend::{Backend, BackendFuture, Capability, PrintJob, Printer},
    error::{ErrorKind, PdfExportError},
    export::build_pdf,
    options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions},
};

/// Embedded so a text export needs no app font; the same bytes the core path passes to `build_pdf`.
const FONT: &[u8] = include_bytes!("../../assets/export-test-font.ttf");

/// The Android backend. It holds no state: v1 has no JNI call sites.
#[derive(Default)]
pub struct AndroidBackend;

impl AndroidBackend {
    /// A backend ready to answer; host tests construct this directly to exercise the refusals.
    pub fn new() -> Self {
        Self
    }
}

/// The refusal every Java-bound capability answers in v1, resolved at once.
fn not_supported<T>() -> BackendFuture<'static, Result<T, PdfExportError>> {
    Box::pin(async { Err(PdfExportError::new(ErrorKind::NotSupported)) })
}

impl Backend for AndroidBackend {
    fn supports(&self, capability: Capability) -> bool {
        matches!(capability, Capability::CreatePdf | Capability::SaveToPath)
    }

    // The trait's answer borrows only `&self`, so the arguments cannot live in the returned
    // future; each is cloned into it. One copy per export is nothing next to encoding the pages.
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
            let bytes = build_pdf(&source, &options, Some(FONT)).await?;
            super::write_pdf_to_path(&bytes, &path).await
        })
    }

    /// No storage picker an NSH app can reach without Java, so v1 refuses rather than guess a path.
    fn pick_save_path(
        &self,
        _suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
        not_supported()
    }

    /// No HTML engine on Android in v1.
    fn convert_html(
        &self,
        _html: &str,
        _options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        not_supported()
    }

    /// PrintManager needs a Java PrintDocumentAdapter an NSH app cannot ship.
    fn print_pdf(
        &self,
        _pdf: &[u8],
        _options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
        not_supported()
    }

    /// Printers follow printing, which is refused, so there is nothing to list.
    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
        not_supported()
    }

    /// No call outlives its own future in v1, so a cancel always resolves.
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
    use crate::options::PageText;

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
        PdfSource {
            kind: Default::default(),
            title: "Report".to_owned(),
            pages: vec![PageText {
                lines: vec!["first line".to_owned(), "second line".to_owned()],
            }],
        }
    }

    #[test]
    fn supports_reports_the_v1_matrix() {
        let backend = AndroidBackend::new();
        assert!(backend.supports(Capability::CreatePdf));
        assert!(backend.supports(Capability::SaveToPath));
        for unsupported in [
            Capability::SaveDialog,
            Capability::ConvertHtml,
            Capability::Print,
            Capability::ListPrinters,
        ] {
            assert!(!backend.supports(unsupported));
        }
    }

    #[test]
    fn print_pdf_refuses_with_the_stable_code() {
        let Err(error) =
            block_on(AndroidBackend::new().print_pdf(b"%PDF".as_slice(), &PrintOptions::default()))
        else {
            panic!("printing must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn pick_save_path_refuses_with_the_stable_code() {
        let Err(error) = block_on(AndroidBackend::new().pick_save_path("report.pdf")) else {
            panic!("the picker must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn list_printers_refuses_with_the_stable_code() {
        let Err(error) = block_on(AndroidBackend::new().list_printers()) else {
            panic!("listing must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn convert_html_refuses_with_the_stable_code() {
        let Err(error) =
            block_on(AndroidBackend::new().convert_html("<p>Report</p>", &HtmlOptions::default()))
        else {
            panic!("conversion must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn create_pdf_passes_the_crate_font_to_the_engine() {
        let backend = AndroidBackend::new();
        let source = one_page();
        let Ok(bytes) = block_on(backend.create_pdf(&source, &ExportOptions::default())) else {
            panic!("creation must build");
        };
        assert!(!bytes.is_empty());
    }

    #[test]
    fn save_pdf_writes_through_missing_parents() {
        let root = std::env::temp_dir().join("nsh-pdf-export-android-save");
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("nested").join("doc.pdf");
        let backend = AndroidBackend::new();
        let source = one_page();
        let Ok(saved) = block_on(backend.save_pdf(&source, &ExportOptions::default(), &path))
        else {
            panic!("the save must write");
        };
        assert_eq!(saved, path);
        assert!(
            !std::fs::read(&path)
                .expect("the document must exist")
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_resolves_with_nothing_running() {
        let Ok(()) = block_on(AndroidBackend::new().cancel()) else {
            panic!("cancel must resolve");
        };
    }
}
