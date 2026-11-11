//! The platform backends, one per target, and the helpers they share: the save dialog and the
//! save-to-path writer. Each bridge compiles on every host, so its refusals stay testable
//! everywhere; a target with no bridge of its own gets [`UnsupportedBackend`].

// Each bridge module is public so host tests construct its backend directly, as the platform
// matrix has it; a target's own backend is the one `default_backend` hands out.
pub mod android;
pub mod apple;
pub mod linux;
pub mod web;
pub mod windows;

use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use nsh_core::file_dialog::{FileFilter, SaveDialog};
use nsh_core::prelude::*;

use crate::{
    backend::{Backend, BackendFuture, Capability, PrintJob, Printer},
    error::{ErrorKind, PdfExportError},
    export::build_pdf,
    options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions},
};

// Embedded so a text export needs no app font; the same bytes every bridge passes to build_pdf.
// Copied from .refs/printpdf/examples/assets/fonts/RobotoMedium.ttf (printpdf 0.12.8).
const FONT: &[u8] = include_bytes!("../../assets/export-test-font.ttf");

/// The backend of the target this build runs on.
pub fn default_backend() -> &'static dyn Backend {
    #[cfg(target_vendor = "apple")]
    {
        static BACKEND: LazyLock<apple::AppleBackend> = LazyLock::new(apple::AppleBackend::new);
        &*BACKEND
    }
    #[cfg(all(target_os = "android", not(target_vendor = "apple")))]
    {
        static BACKEND: LazyLock<android::AndroidBackend> =
            LazyLock::new(android::AndroidBackend::new);
        &*BACKEND
    }
    #[cfg(all(target_os = "windows", not(target_vendor = "apple")))]
    {
        static BACKEND: LazyLock<windows::WindowsBackend> =
            LazyLock::new(windows::WindowsBackend::new);
        &*BACKEND
    }
    #[cfg(all(
        target_os = "linux",
        not(target_vendor = "apple"),
        not(target_os = "android"),
        not(target_os = "windows")
    ))]
    {
        static BACKEND: LazyLock<linux::LinuxBackend> = LazyLock::new(linux::LinuxBackend::new);
        &*BACKEND
    }
    #[cfg(all(
        target_arch = "wasm32",
        not(target_vendor = "apple"),
        not(target_os = "android"),
        not(target_os = "windows"),
        not(target_os = "linux")
    ))]
    {
        static BACKEND: LazyLock<web::WebBackend> = LazyLock::new(web::WebBackend::new);
        &*BACKEND
    }
    #[cfg(not(any(
        target_vendor = "apple",
        target_os = "android",
        target_os = "windows",
        target_os = "linux",
        target_arch = "wasm32"
    )))]
    {
        static BACKEND: LazyLock<UnsupportedBackend> = LazyLock::new(UnsupportedBackend::new);
        &*BACKEND
    }
}

/// The backend of a target with no bridge of its own. Creation and saving still work — they run
/// the crate's own engine — while every platform-bound capability is not available.
pub struct UnsupportedBackend;

impl UnsupportedBackend {
    /// A backend ready to answer; host tests construct this directly to exercise the refusals.
    pub fn new() -> Self {
        Self
    }
}

impl Default for UnsupportedBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// The refusal every platform-bound capability answers.
fn unavailable<T>() -> Result<T, PdfExportError> {
    Err(PdfExportError::new(ErrorKind::NotAvailable))
}

impl Backend for UnsupportedBackend {
    fn supports(&self, capability: Capability) -> bool {
        matches!(capability, Capability::CreatePdf | Capability::SaveToPath)
    }

    fn create_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        // The returned future may not borrow arguments, so it owns its data.
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
        // The returned future may not borrow arguments, so it owns its data.
        let source = source.clone();
        let options = options.clone();
        let path = path.to_path_buf();
        Box::pin(async move {
            let bytes = build_pdf(&source, &options, Some(FONT)).await?;
            write_pdf_to_path(&bytes, &path).await
        })
    }

    fn pick_save_path(
        &self,
        _suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
        Box::pin(std::future::ready(unavailable()))
    }

    fn convert_html(
        &self,
        _html: &str,
        _options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        Box::pin(std::future::ready(unavailable()))
    }

    fn print_pdf(
        &self,
        _pdf: &[u8],
        _options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
        Box::pin(std::future::ready(unavailable()))
    }

    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
        Box::pin(std::future::ready(unavailable()))
    }

    fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>> {
        // Creation is pure computation that settles on its own; there is nothing to cancel,
        // and accepting the request keeps every cancel resolution coded and immediate.
        Box::pin(std::future::ready(Ok(())))
    }
}

/// Shows the system's save dialog, desktops only: `Ok(None)` is the user cancelling, and a
/// dialog that cannot show — any platform without file dialogs — is not available.
pub(crate) async fn save_dialog_path(
    suggested_name: &str,
) -> Result<Option<PathBuf>, PdfExportError> {
    let dialog = SaveDialog::new()
        .file_name(suggested_name)
        .filter(FileFilter::new("PDF", ["pdf"]));
    // The dialog's own error text stays inside; no message leaves with a path in it.
    pick_save_path(dialog)
        .await
        .map_err(|_| PdfExportError::new(ErrorKind::NotAvailable))
}

/// Writes `bytes` to `path`, creating missing parents. Where a runtime is running, the write
/// goes through its blocking pool, off the async threads; with none (a bare test, the web), it
/// runs inline. Every write failure is `WriteFailed`; a panicked pool writer is the backend's
/// own failure.
pub(crate) async fn write_pdf_to_path(
    bytes: &[u8],
    path: &Path,
) -> Result<PathBuf, PdfExportError> {
    let path = path.to_path_buf();
    let bytes = bytes.to_vec();
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            return match handle
                .spawn_blocking(move || write_file(&path, &bytes))
                .await
            {
                Ok(written) => written,
                // A panicked writer: the join fails, and the write is the backend's to answer for.
                Err(_) => Err(PdfExportError::new(ErrorKind::Backend)),
            };
        }
    }
    write_file(&path, &bytes)
}

/// The blocking write itself; the OS error text never enters the message.
fn write_file(path: &Path, bytes: &[u8]) -> Result<PathBuf, PdfExportError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent).map_err(|_| PdfExportError::new(ErrorKind::WriteFailed))?;
    }
    std::fs::write(path, bytes).map_err(|_| PdfExportError::new(ErrorKind::WriteFailed))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    use super::*;

    /// Runs `future` to its single answer on the test thread; these futures never yield
    /// `Pending` before their answer.
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
        PdfSource::text("Report", vec!["Hello PDF".to_owned()])
    }

    #[test]
    fn unsupported_supports_creation_only() {
        let backend = UnsupportedBackend::new();
        assert!(backend.supports(Capability::CreatePdf));
        assert!(backend.supports(Capability::SaveToPath));
        for capability in [
            Capability::SaveDialog,
            Capability::ConvertHtml,
            Capability::Print,
            Capability::ListPrinters,
        ] {
            assert!(!backend.supports(capability));
        }
    }

    #[test]
    fn unsupported_platform_capabilities_are_not_available() {
        let backend = UnsupportedBackend::new();
        for error in [
            block_on(backend.pick_save_path("doc.pdf")).unwrap_err(),
            block_on(backend.convert_html("<p>hi</p>", &HtmlOptions::default())).unwrap_err(),
            block_on(backend.print_pdf(b"%PDF-1.7", &PrintOptions::default())).unwrap_err(),
            block_on(backend.list_printers()).unwrap_err(),
        ] {
            assert_eq!(error.kind(), ErrorKind::NotAvailable);
            assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_AVAILABLE");
        }
        assert_eq!(
            block_on(backend.cancel()),
            Ok(()),
            "a cancel with nothing running still resolves"
        );
    }

    #[tokio::test]
    async fn unsupported_creation_builds_real_bytes() {
        let bytes = UnsupportedBackend::new()
            .create_pdf(&one_page(), &ExportOptions::default())
            .await
            .expect("creation runs the crate's own engine");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[tokio::test]
    async fn write_pdf_to_path_creates_parents_and_reports_write_failures() {
        let root = std::env::temp_dir().join("nsh-pdf-export-core-write");
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("nested").join("doc.pdf");
        let saved = write_pdf_to_path(b"%PDF-1.7", &path)
            .await
            .expect("the write creates its parents");
        assert_eq!(saved, path);
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));

        // A path under a file cannot be written to, whatever the platform calls the error.
        let under_file = path.join("deeper.pdf");
        let error = write_pdf_to_path(b"%PDF-1.7", &under_file)
            .await
            .expect_err("the write reports itself");
        assert_eq!(error.kind(), ErrorKind::WriteFailed);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_WRITE_FAILED");
        let _ = std::fs::remove_dir_all(&root);
    }
}
