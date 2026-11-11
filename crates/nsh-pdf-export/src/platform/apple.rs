//! The Apple bridge (iOS and macOS). PDF creation and saving run the crate's
//! own engine on both; the surfaces split: macOS gets the save dialog, iOS the
//! print sheet, and neither converts HTML in v1.

use std::path::{Path, PathBuf};

use crate::{
    backend::{Backend, BackendFuture, Capability, PrintJob, Printer},
    error::{ErrorKind, PdfExportError},
    export::build_pdf,
    options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions},
};

/// Embedded so a text export needs no app font; the same bytes the core path passes to `build_pdf`.
const FONT: &[u8] = include_bytes!("../../assets/export-test-font.ttf");

/// The Apple backend. Holds no state: the print sheet is UIKit's shared
/// controller, reached through the main thread each time printing starts.
#[derive(Default)]
pub struct AppleBackend;

impl AppleBackend {
    /// A backend ready to answer; host tests construct this directly to exercise the refusals.
    pub fn new() -> Self {
        Self
    }
}

/// The refusal every unavailable capability answers with, resolved at once.
fn not_supported<T>() -> BackendFuture<'static, Result<T, PdfExportError>> {
    Box::pin(async { Err(PdfExportError::new(ErrorKind::NotSupported)) })
}

impl Backend for AppleBackend {
    fn supports(&self, capability: Capability) -> bool {
        match capability {
            Capability::CreatePdf | Capability::SaveToPath => true,
            Capability::SaveDialog => cfg!(target_os = "macos"),
            Capability::ConvertHtml => false,
            // The print sheet exists on iOS only, and no printer query without
            // the picker in v1, so listing is refused on both Apple targets.
            Capability::Print => cfg!(target_os = "ios"),
            Capability::ListPrinters => false,
        }
    }

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
            super::write_pdf_to_path(&pdf, &path).await
        })
    }

    fn pick_save_path(
        &self,
        _suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
        #[cfg(target_os = "macos")]
        {
            let suggested_name = _suggested_name.to_owned();
            Box::pin(async move { super::save_dialog_path(&suggested_name).await })
        }
        #[cfg(not(target_os = "macos"))]
        {
            // iOS apps see no filesystem a dialog could browse; v1 refuses.
            not_supported()
        }
    }

    /// No HTML engine on Apple targets in v1.
    fn convert_html(
        &self,
        _html: &str,
        _options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        not_supported()
    }

    fn print_pdf(
        &self,
        _pdf: &[u8],
        options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
        // Checked everywhere, so a bad value is InvalidArgument even where the
        // platform below refuses; the sheet's own control carries the count on.
        let copies = options.copies.unwrap_or(1);
        if !(1..=999).contains(&copies) {
            return Box::pin(async { Err(PdfExportError::new(ErrorKind::InvalidArgument)) });
        }
        #[cfg(target_os = "ios")]
        {
            let printer = options.printer_name.clone();
            let job_name = options.job_name.clone();
            let pdf = _pdf.to_vec();
            Box::pin(
                async move { sheet::print(&pdf, printer.as_deref(), job_name.as_deref()).await },
            )
        }
        #[cfg(not(target_os = "ios"))]
        {
            // macOS printing waits past v1 (docs/decisions.md).
            not_supported()
        }
    }

    /// UIPrintInteractionController offers no printer query without its picker
    /// in v1, and macOS does not print at all, so both targets refuse.
    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
        not_supported()
    }

    fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>> {
        #[cfg(target_os = "ios")]
        {
            sheet::dismiss()
        }
        #[cfg(not(target_os = "ios"))]
        {
            // No macOS call outlives its own future in v1, so a cancel resolves at once.
            Box::pin(async { Ok(()) })
        }
    }
}

/// The UIKit print sheet. Everything here runs only under `target_os = "ios"`;
/// the other Apple target answers `NotSupported` in `Backend for AppleBackend`.
#[cfg(target_os = "ios")]
mod sheet {
    use super::*;
    use block2::RcBlock;
    use dispatch2::run_on_main;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_foundation::{NSData, NSError, NSString};
    use objc2_ui_kit::{UIPrintInfo, UIPrintInteractionController};
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    /// The wait for the sheet's answer never outlives this bound, so a sheet
    /// left open cannot hang the operation; the front's shorter default bound
    /// and its `cancel` reach it first in normal use.
    const SHEET_WAIT: Duration = Duration::from_secs(300);

    /// A cancel waits this long for the dismissal before giving up on it.
    const DISMISS_WAIT: Duration = Duration::from_secs(5);

    /// The sheet reports no system job id, so a process-wide counter stands in.
    static JOB_IDS: AtomicI64 = AtomicI64::new(0);

    /// What the sheet's completion handler reports back.
    enum Answer {
        Printed,
        Failed,
        Cancelled,
    }

    /// What presenting the shared sheet came to.
    enum Shown {
        Sheet,
        /// The singleton controller refused: a sheet is already up.
        Refused,
        NoPrinter,
    }

    /// Presents the shared sheet with `pdf` and waits for its single answer.
    pub(super) async fn print(
        pdf: &[u8],
        printer: Option<&str>,
        job_name: Option<&str>,
    ) -> Result<PrintJob, PdfExportError> {
        if printer.is_some() {
            // No printer lookup without the picker in v1, so naming one is not
            // allowed here; the sheet's own picker chooses the printer.
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
        if pdf.is_empty() {
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
        let pdf = pdf.to_vec();
        let job_name = job_name.map(str::to_owned);
        // UIKit is main-thread, the dispatch to it is synchronous, and the wait
        // blocks, so the whole present runs on the blocking pool inside one
        // bound; a panic resolves Backend and a main thread that never services
        // its queue resolves Timeout instead of wedging the future.
        let presenting =
            tokio::task::spawn_blocking(move || present_and_wait(&pdf, job_name.as_deref()));
        match tokio::time::timeout(SHEET_WAIT, presenting).await {
            Ok(Ok(result)) => result,
            // The blocking task itself failed; it cannot be recovered from here.
            Ok(Err(_)) => Err(PdfExportError::new(ErrorKind::Backend)),
            Err(_) => {
                // The task may sit in the dispatch forever; a dismissal sent
                // the same way would park the same way, so resolve without it.
                Err(PdfExportError::new(ErrorKind::Timeout))
            }
        }
    }

    fn present_and_wait(pdf: &[u8], job_name: Option<&str>) -> Result<PrintJob, PdfExportError> {
        let (answers, answer) = mpsc::channel::<Answer>();
        // run_on_main takes a Send closure and blocks are not Send, so the
        // block is built on the main thread and only the sender crosses.
        let shown = run_on_main(move |mtm| {
            if !UIPrintInteractionController::isPrintingAvailable(mtm) {
                return Shown::NoPrinter;
            }
            let sheet = UIPrintInteractionController::sharedPrintController(mtm);
            let info = UIPrintInfo::printInfo(mtm);
            if let Some(name) = job_name {
                info.setJobName(&NSString::from_str(name));
            }
            sheet.setPrintInfo(Some(&info));
            // SAFETY: the initializer copies the bytes, and every Objective-C
            // object is an AnyObject — the cast only widens the static type.
            let item = unsafe {
                let data = NSData::dataWithBytes_length(pdf.as_ptr().cast::<c_void>(), pdf.len());
                Retained::cast_unchecked::<AnyObject>(data)
            };
            // SAFETY: a plain NSData is one of printingItem's documented kinds.
            unsafe { sheet.setPrintingItem(Some(&item)) };
            // Apple reports failure as completed with a non-nil error and
            // cancellation as completed without one, so the error is checked
            // before the flag.
            let completion: RcBlock<
                dyn Fn(NonNull<UIPrintInteractionController>, Bool, *mut NSError),
            > = RcBlock::new(
                move |_sheet: NonNull<UIPrintInteractionController>,
                      printed: Bool,
                      error: *mut NSError| {
                    let answer = if !error.is_null() {
                        Answer::Failed
                    } else if printed.as_bool() {
                        Answer::Printed
                    } else {
                        Answer::Cancelled
                    };
                    let _ = answers.send(answer);
                },
            );
            // SAFETY: UIKit copies the block at present time and holds that
            // copy until the sheet ends, so the RcBlock may drop when this
            // closure returns; the copy fires later.
            let shown = unsafe {
                sheet.presentAnimated_completionHandler(true, RcBlock::as_ptr(&completion))
            };
            if shown { Shown::Sheet } else { Shown::Refused }
        });
        match shown {
            Shown::NoPrinter => Err(PdfExportError::new(ErrorKind::NoPrinter)),
            Shown::Refused => Err(PdfExportError::new(ErrorKind::InProgress)),
            Shown::Sheet => match answer.recv_timeout(SHEET_WAIT) {
                Ok(Answer::Printed) => Ok(PrintJob {
                    id: JOB_IDS.fetch_add(1, Ordering::Relaxed) + 1,
                }),
                Ok(Answer::Failed) => Err(PdfExportError::new(ErrorKind::PrintFailed)),
                Ok(Answer::Cancelled) => Err(PdfExportError::new(ErrorKind::Cancelled)),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // The bound in `print` resolves the caller first; this arm
                    // only frees the thread. A dismissal dispatched to the
                    // same wedged main queue would park it again.
                    Err(PdfExportError::new(ErrorKind::Timeout))
                }
                // Cannot happen while the block's copy holds a sender; kept so
                // the wait still resolves exactly once if it ever does.
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Err(PdfExportError::new(ErrorKind::Backend))
                }
            },
        }
    }

    /// Closes the shared sheet if one is up; UIKit treats it as a no-op when
    /// none is. Blocks until the main thread has run the dismissal.
    fn dismiss_now() {
        run_on_main(|mtm| {
            UIPrintInteractionController::sharedPrintController(mtm).dismissAnimated(false);
        });
    }

    /// Attempts to close the sheet, bounded: a wedged main thread cannot hang
    /// the cancel as well.
    pub(super) fn dismiss() -> BackendFuture<'static, Result<(), PdfExportError>> {
        Box::pin(async {
            let dismissed = tokio::task::spawn_blocking(dismiss_now);
            match tokio::time::timeout(DISMISS_WAIT, dismissed).await {
                Ok(Ok(())) => Ok(()),
                // The blocking task itself failed; it cannot be recovered from here.
                Ok(Err(_)) => Err(PdfExportError::new(ErrorKind::Backend)),
                Err(_) => Err(PdfExportError::new(ErrorKind::Timeout)),
            }
        })
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

    /// Runs `future` to its single answer on the test thread.
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
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
        let backend = AppleBackend::new();
        assert!(backend.supports(Capability::CreatePdf));
        assert!(backend.supports(Capability::SaveToPath));
        assert_eq!(
            backend.supports(Capability::SaveDialog),
            cfg!(target_os = "macos")
        );
        assert!(!backend.supports(Capability::ConvertHtml));
        assert_eq!(backend.supports(Capability::Print), cfg!(target_os = "ios"));
        // Listing is refused on both Apple targets, iOS printing or not.
        assert!(!backend.supports(Capability::ListPrinters));
    }

    #[test]
    fn print_pdf_refuses_with_the_stable_code() {
        let Err(error) =
            block_on(AppleBackend::new().print_pdf(b"%PDF".as_slice(), &PrintOptions::default()))
        else {
            panic!("printing must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn print_pdf_rejects_copies_out_of_range_before_the_platform_answers() {
        for copies in [0, 1000] {
            let options = PrintOptions {
                copies: Some(copies),
                ..PrintOptions::default()
            };
            let Err(error) = block_on(AppleBackend::new().print_pdf(b"%PDF".as_slice(), &options))
            else {
                panic!("copies {copies} must refuse");
            };
            assert!(matches!(error.kind(), ErrorKind::InvalidArgument));
            assert_eq!(error.code(), "ERR_PDF_EXPORT_INVALID_ARGUMENT");
        }
    }

    #[test]
    fn list_printers_refuses_with_the_stable_code() {
        let Err(error) = block_on(AppleBackend::new().list_printers()) else {
            panic!("listing must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn convert_html_refuses_with_the_stable_code() {
        let Err(error) =
            block_on(AppleBackend::new().convert_html("<p>Report</p>", &HtmlOptions::default()))
        else {
            panic!("conversion must refuse");
        };
        assert!(matches!(error.kind(), ErrorKind::NotSupported));
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[test]
    fn create_pdf_passes_the_crate_font_to_the_engine() {
        let backend = AppleBackend::new();
        let source = one_page();
        let Ok(bytes) = block_on(backend.create_pdf(&source, &ExportOptions::default())) else {
            panic!("creation must build");
        };
        assert!(!bytes.is_empty());
    }

    #[test]
    fn save_pdf_writes_through_the_shared_helper() {
        let root = std::env::temp_dir().join("nsh-pdf-export-apple-save");
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("nested").join("doc.pdf");
        let backend = AppleBackend::new();
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
        let Ok(()) = block_on(AppleBackend::new().cancel()) else {
            panic!("cancel must resolve");
        };
    }
}
