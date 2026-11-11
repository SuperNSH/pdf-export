//! The Linux backend. Printing and printer discovery call libcups through
//! hand-declared FFI (no `-sys` crate); PDF creation, saving and the save
//! dialog ride the shared helpers in [`super`]. The module compiles on
//! every host so its off-Linux behaviour stays testable off Linux.

use std::path::{Path, PathBuf};

use crate::backend::{Backend, BackendFuture, Capability, PrintJob, Printer};
use crate::error::{ErrorKind, PdfExportError};
use crate::export::build_pdf;
use crate::options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions};

/// Embedded so a text export needs no app font; the same bytes the core path passes to `build_pdf`.
const FONT: &[u8] = include_bytes!("../../assets/export-test-font.ttf");

/// The Linux backend. Holds no state; the real libcups calls exist only
/// under `target_os = "linux"`.
pub struct LinuxBackend;

impl LinuxBackend {
    /// Builds the backend.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for LinuxBackend {
    fn supports(&self, capability: Capability) -> bool {
        match capability {
            Capability::CreatePdf | Capability::SaveToPath | Capability::SaveDialog => true,
            Capability::ConvertHtml => false,
            Capability::Print | Capability::ListPrinters => cfg!(target_os = "linux"),
        }
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
        suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
        let suggested_name = suggested_name.to_owned();
        Box::pin(async move { super::save_dialog_path(&suggested_name).await })
    }

    fn convert_html(
        &self,
        _html: &str,
        _options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        Box::pin(async { Err(PdfExportError::new(ErrorKind::NotSupported)) })
    }

    fn print_pdf(
        &self,
        pdf: &[u8],
        options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
        let copies = options.copies.unwrap_or(1);
        if !(1..=999).contains(&copies) {
            return Box::pin(async { Err(PdfExportError::new(ErrorKind::InvalidArgument)) });
        }
        let printer = options.printer_name.clone();
        let job_name = options.job_name.clone();
        let pdf = pdf.to_vec();
        Box::pin(async move { dispatch_print(pdf, printer, job_name, copies).await })
    }

    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
        Box::pin(async move { dispatch_list_printers().await })
    }

    fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>> {
        // A CUPS request in flight cannot be pulled back mid-call in v1; the
        // 30 s call bound resolves a stuck request on its own.
        Box::pin(async { Ok(()) })
    }
}

/// One blocking CUPS call resolves within this bound, so a hung daemon
/// surfaces as `Timeout` from the bridge even with the front bound disabled.
#[cfg(target_os = "linux")]
const CALL_BOUND: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(target_os = "linux")]
async fn dispatch_print(
    pdf: Vec<u8>,
    printer: Option<String>,
    job_name: Option<String>,
    copies: u32,
) -> Result<PrintJob, PdfExportError> {
    // CUPS calls block on the daemon, so they run on the blocking pool; a
    // panic there resolves to `PrintFailed` instead of taking the runtime down.
    let job = tokio::task::spawn_blocking(move || {
        cups::print(&pdf, printer.as_deref(), job_name.as_deref(), copies)
    });
    match tokio::time::timeout(CALL_BOUND, job).await {
        Ok(Ok(job)) => job.map(|id| PrintJob { id: i64::from(id) }),
        Ok(Err(_join)) => Err(PdfExportError::new(ErrorKind::PrintFailed)),
        Err(_elapsed) => Err(PdfExportError::new(ErrorKind::Timeout)),
    }
}

#[cfg(not(target_os = "linux"))]
async fn dispatch_print(
    _pdf: Vec<u8>,
    _printer: Option<String>,
    _job_name: Option<String>,
    _copies: u32,
) -> Result<PrintJob, PdfExportError> {
    Err(PdfExportError::new(ErrorKind::NotSupported))
}

#[cfg(target_os = "linux")]
async fn dispatch_list_printers() -> Result<Vec<Printer>, PdfExportError> {
    // The same pool and bound as printing: listing talks to the same daemon.
    let list = tokio::task::spawn_blocking(cups::list);
    match tokio::time::timeout(CALL_BOUND, list).await {
        Ok(joined) => joined.map_err(|_| PdfExportError::new(ErrorKind::Backend)),
        Err(_elapsed) => Err(PdfExportError::new(ErrorKind::Timeout)),
    }
}

#[cfg(not(target_os = "linux"))]
async fn dispatch_list_printers() -> Result<Vec<Printer>, PdfExportError> {
    Err(PdfExportError::new(ErrorKind::NotSupported))
}

#[cfg(target_os = "linux")]
mod cups {
    //! Hand-declared libcups FFI, checked against the CUPS 2.x headers that
    //! Ubuntu's libcups2-dev ships. Only these safe wrappers leave the mod.

    use std::ffi::{CStr, CString, c_char, c_int};
    use std::ptr::{null, null_mut};

    use crate::backend::Printer;
    use crate::error::{ErrorKind, PdfExportError};

    /// `cups_option_t`; the field order is the ABI, do not reorder.
    #[repr(C)]
    struct CupsOption {
        name: *mut c_char,
        value: *mut c_char,
    }

    /// `cups_dest_t`; the field order is the ABI, do not reorder.
    #[repr(C)]
    struct CupsDest {
        name: *mut c_char,
        instance: *mut c_char,
        is_default: c_int,
        num_options: c_int,
        options: *mut CupsOption,
    }

    /// Opaque `http_t`; only ever passed null, which is `CUPS_HTTP_DEFAULT`.
    #[repr(C)]
    struct Http {
        _private: [u8; 0],
    }

    /// `cupsStartDocument` and `cupsWriteRequestData` report progress with it.
    const HTTP_STATUS_CONTINUE: c_int = 100;
    /// `cupsFinishDocument` succeeds with this IPP status.
    const IPP_STATUS_OK: c_int = 0;
    const FORMAT_PDF: &CStr = c"application/pdf";
    const DEFAULT_JOB_NAME: &str = "PDF document";
    /// CUPS streams fine at any chunk size; 64 KiB keeps memory flat.
    const WRITE_CHUNK: usize = 64 * 1024;

    #[link(name = "cups")]
    unsafe extern "C" {
        fn cupsGetDests(dests: *mut *mut CupsDest) -> c_int;
        fn cupsGetDest(
            name: *const c_char,
            instance: *const c_char,
            num_dests: c_int,
            dests: *mut CupsDest,
        ) -> *mut CupsDest;
        fn cupsCreateJob(
            http: *mut Http,
            name: *const c_char,
            title: *const c_char,
            num_options: c_int,
            options: *mut CupsOption,
        ) -> c_int;
        fn cupsStartDocument(
            http: *mut Http,
            name: *const c_char,
            job_id: c_int,
            docname: *const c_char,
            format: *const c_char,
            last_document: c_int,
        ) -> c_int;
        fn cupsWriteRequestData(http: *mut Http, buffer: *const c_char, length: usize) -> c_int;
        fn cupsFinishDocument(http: *mut Http, name: *const c_char) -> c_int;
        fn cupsAddOption(
            name: *const c_char,
            value: *const c_char,
            num_options: c_int,
            options: *mut *mut CupsOption,
        ) -> c_int;
        fn cupsFreeOptions(num_options: c_int, options: *mut CupsOption);
        fn cupsFreeDests(num_dests: c_int, dests: *mut CupsDest);
        fn cupsCancelJob(name: *const c_char, job_id: c_int) -> c_int;
        fn cupsLastError() -> c_int;
    }

    /// Every printer CUPS knows and which one is the default. No reachable
    /// destination is an empty list, not an error: callers list to display.
    pub(super) fn list() -> Vec<Printer> {
        let mut dests: *mut CupsDest = null_mut();
        // SAFETY: writes only through the out-pointer we own.
        let count = unsafe { cupsGetDests(&mut dests) };
        let mut printers = Vec::new();
        if !dests.is_null() {
            for index in 0..count {
                // SAFETY: index stays below count, so it addresses CUPS' array.
                let dest = unsafe { &*dests.add(index as usize) };
                if !dest.name.is_null() {
                    // SAFETY: CUPS owns the string and NUL-terminates it.
                    let name = unsafe { CStr::from_ptr(dest.name) };
                    printers.push(Printer {
                        name: name.to_string_lossy().into_owned(),
                        is_default: dest.is_default != 0,
                    });
                }
            }
            // SAFETY: dests is CUPS' array holding exactly count entries.
            unsafe { cupsFreeDests(count, dests) };
        }
        printers
    }

    /// Sends `pdf` to `printer` (the default when `None`) as one document
    /// and returns the job id CUPS assigned.
    pub(super) fn print(
        pdf: &[u8],
        printer: Option<&str>,
        job_name: Option<&str>,
        copies: u32,
    ) -> Result<i32, PdfExportError> {
        if pdf.is_empty() {
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
        let printer = printer
            .map(CString::new)
            .transpose()
            .map_err(|_| PdfExportError::new(ErrorKind::InvalidArgument))?;
        let title = CString::new(job_name.unwrap_or(DEFAULT_JOB_NAME))
            .map_err(|_| PdfExportError::new(ErrorKind::InvalidArgument))?;
        let name = match destination(printer.as_deref())? {
            Some(name) => name,
            None => return Err(PdfExportError::new(ErrorKind::NoPrinter)),
        };

        let mut options: *mut CupsOption = null_mut();
        let num_options = if copies == 1 {
            0
        } else {
            // A formatted u32 holds no interior NUL, so this cannot fail.
            let value = CString::new(copies.to_string()).expect("no NUL in a formatted u32");
            // SAFETY: options starts as CUPS' empty list (null) per contract.
            unsafe { cupsAddOption(c"copies".as_ptr(), value.as_ptr(), 0, &mut options) }
        };

        // SAFETY: null http is CUPS_HTTP_DEFAULT; the strings outlive the call.
        let job_id = unsafe {
            cupsCreateJob(
                null_mut(),
                name.as_ptr(),
                title.as_ptr(),
                num_options,
                options,
            )
        };
        // SAFETY: options holds exactly the num_options entries added above.
        unsafe { cupsFreeOptions(num_options, options) };
        if job_id <= 0 {
            // SAFETY: reads CUPS' thread-local last error; no preconditions.
            return Err(failed(unsafe { cupsLastError() }));
        }

        // SAFETY: null http is CUPS_HTTP_DEFAULT; one document, so last = 1.
        let status = unsafe {
            cupsStartDocument(
                null_mut(),
                name.as_ptr(),
                job_id,
                title.as_ptr(),
                FORMAT_PDF.as_ptr(),
                1,
            )
        };
        if status != HTTP_STATUS_CONTINUE {
            return Err(cancelled(name.as_ptr(), job_id, status));
        }
        for chunk in pdf.chunks(WRITE_CHUNK) {
            // SAFETY: the pointer and length describe this borrowed chunk.
            let status =
                unsafe { cupsWriteRequestData(null_mut(), chunk.as_ptr().cast(), chunk.len()) };
            if status != HTTP_STATUS_CONTINUE {
                return Err(cancelled(name.as_ptr(), job_id, status));
            }
        }
        // SAFETY: null http is CUPS_HTTP_DEFAULT.
        let status = unsafe { cupsFinishDocument(null_mut(), name.as_ptr()) };
        if status != IPP_STATUS_OK {
            return Err(cancelled(name.as_ptr(), job_id, status));
        }
        Ok(job_id)
    }

    /// The printer to target: `printer` by name, else CUPS' default. `None`
    /// also covers a named printer CUPS does not know.
    fn destination(printer: Option<&CStr>) -> Result<Option<CString>, PdfExportError> {
        let mut dests: *mut CupsDest = null_mut();
        // SAFETY: writes only through the out-pointer we own.
        let count = unsafe { cupsGetDests(&mut dests) };
        if dests.is_null() {
            return Err(PdfExportError::new(ErrorKind::NoPrinter));
        }
        // SAFETY: reads CUPS' array; name and instance are null or C strings.
        let dest = unsafe {
            cupsGetDest(
                printer.map_or(null(), |name| name.as_ptr()),
                null(),
                count,
                dests,
            )
        };
        let name = if dest.is_null() {
            None
        } else {
            // SAFETY: dest is an entry of dests; its name is NUL-terminated.
            let ptr = unsafe { (*dest).name };
            if ptr.is_null() {
                None
            } else {
                // SAFETY: copied out before the array is freed below.
                Some(unsafe { CStr::from_ptr(ptr) }.to_owned())
            }
        };
        // SAFETY: dests is CUPS' array holding exactly count entries.
        unsafe { cupsFreeDests(count, dests) };
        Ok(name)
    }

    /// Reports `status` as `PrintFailed` after a best-effort job cancel; a
    /// failed cancel of a broken job is not itself worth an error.
    fn cancelled(name: *const c_char, job_id: i32, status: c_int) -> PdfExportError {
        // SAFETY: name still points at a valid C string for this call.
        unsafe { cupsCancelJob(name, job_id) };
        failed(status)
    }

    fn failed(status: c_int) -> PdfExportError {
        PdfExportError::with_status(ErrorKind::PrintFailed, i64::from(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print_options(copies: Option<u32>) -> PrintOptions {
        PrintOptions {
            printer_name: None,
            job_name: None,
            copies,
        }
    }

    #[test]
    fn supports_mirrors_the_v1_matrix() {
        let backend = LinuxBackend::new();
        assert!(backend.supports(Capability::CreatePdf));
        assert!(backend.supports(Capability::SaveToPath));
        assert!(backend.supports(Capability::SaveDialog));
        assert!(!backend.supports(Capability::ConvertHtml));
        assert_eq!(
            backend.supports(Capability::Print),
            cfg!(target_os = "linux")
        );
        assert_eq!(
            backend.supports(Capability::ListPrinters),
            cfg!(target_os = "linux")
        );
    }

    #[tokio::test]
    async fn html_conversion_is_not_supported() {
        let error = LinuxBackend::new()
            .convert_html("<p>hello</p>", &HtmlOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotSupported);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
        assert_eq!(error.status(), None);
    }

    #[tokio::test]
    async fn print_rejects_copies_out_of_range() {
        for copies in [0, 1000] {
            let error = LinuxBackend::new()
                .print_pdf(b"%PDF-1.7", &print_options(Some(copies)))
                .await
                .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
            assert_eq!(error.code(), "ERR_PDF_EXPORT_INVALID_ARGUMENT");
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn print_and_listing_are_not_supported_off_linux() {
        let backend = LinuxBackend::new();
        let error = backend
            .print_pdf(b"%PDF-1.7", &print_options(Some(1)))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotSupported);
        let error = backend.list_printers().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotSupported);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    }

    #[tokio::test]
    async fn cancel_resolves_ok() {
        LinuxBackend::new().cancel().await.unwrap();
    }
}
