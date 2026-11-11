//! The Windows backend. Printing spools the finished PDF through winspool
//! as a RAW job the printer's own firmware renders; discovery asks the
//! spooler. PDF creation, saving and the save dialog ride the shared
//! helpers in [`super`]. The module compiles on every host so its
//! off-Windows behaviour stays testable off Windows.

use std::path::{Path, PathBuf};

use crate::backend::{Backend, BackendFuture, Capability, PrintJob, Printer};
use crate::error::{ErrorKind, PdfExportError};
use crate::export::build_pdf;
use crate::options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions};

/// Embedded so a text export needs no app font; the same bytes the core path passes to `build_pdf`.
const FONT: &[u8] = include_bytes!("../../assets/export-test-font.ttf");

/// The Windows backend. Holds no state; the real winspool calls exist only
/// under `target_os = "windows"`.
pub struct WindowsBackend;

impl WindowsBackend {
    /// Builds the backend.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for WindowsBackend {
    fn supports(&self, capability: Capability) -> bool {
        match capability {
            Capability::CreatePdf | Capability::SaveToPath | Capability::SaveDialog => true,
            Capability::ConvertHtml => false,
            Capability::Print | Capability::ListPrinters => cfg!(target_os = "windows"),
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
        // A spooled RAW job cannot be pulled back mid-write in v1; the front
        // API's timeout bound is what resolves a stuck operation.
        Box::pin(async { Ok(()) })
    }
}

/// One blocking spooler call resolves within this bound, so a stuck spooler
/// surfaces as `Timeout` from the bridge even with the front bound disabled.
#[cfg(target_os = "windows")]
const CALL_BOUND: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(target_os = "windows")]
async fn dispatch_print(
    pdf: Vec<u8>,
    printer: Option<String>,
    job_name: Option<String>,
    copies: u32,
) -> Result<PrintJob, PdfExportError> {
    // Spooler calls block on the service, so they run on the blocking pool;
    // a panic there resolves to `PrintFailed` instead of taking the runtime down.
    let job = tokio::task::spawn_blocking(move || {
        spool::print(&pdf, printer.as_deref(), job_name.as_deref(), copies)
    });
    let job = match tokio::time::timeout(CALL_BOUND, job).await {
        Ok(Ok(job)) => job?,
        Ok(Err(_panic)) => return Err(PdfExportError::new(ErrorKind::PrintFailed)),
        Err(_elapsed) => return Err(PdfExportError::new(ErrorKind::Timeout)),
    };
    Ok(PrintJob { id: i64::from(job) })
}

#[cfg(not(target_os = "windows"))]
async fn dispatch_print(
    _pdf: Vec<u8>,
    _printer: Option<String>,
    _job_name: Option<String>,
    _copies: u32,
) -> Result<PrintJob, PdfExportError> {
    Err(PdfExportError::new(ErrorKind::NotSupported))
}

#[cfg(target_os = "windows")]
async fn dispatch_list_printers() -> Result<Vec<Printer>, PdfExportError> {
    // The same pool and bound as printing: listing talks to the same service.
    let list = tokio::task::spawn_blocking(spool::list);
    match tokio::time::timeout(CALL_BOUND, list).await {
        Ok(Ok(list)) => list,
        Ok(Err(_panic)) => Err(PdfExportError::new(ErrorKind::Backend)),
        Err(_elapsed) => Err(PdfExportError::new(ErrorKind::Timeout)),
    }
}

#[cfg(not(target_os = "windows"))]
async fn dispatch_list_printers() -> Result<Vec<Printer>, PdfExportError> {
    Err(PdfExportError::new(ErrorKind::NotSupported))
}

#[cfg(target_os = "windows")]
mod spool {
    //! winspool RAW passthrough, written against the windows 0.62 bindings.
    //! Only these safe wrappers leave the mod.

    use windows::Win32::Foundation::{ERROR_INVALID_NAME, ERROR_INVALID_PRINTER_NAME};
    use windows::Win32::Graphics::Printing::{
        ClosePrinter, DOC_INFO_1W, EndDocPrinter, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW,
        PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_HANDLE, PRINTER_INFO_4W,
        StartDocPrinterW, WritePrinter,
    };
    use windows::core::{Error as WindowsError, HRESULT, HSTRING, PCWSTR, PWSTR};

    use crate::backend::Printer;
    use crate::error::{ErrorKind, PdfExportError};

    /// Level 4 names local printers and connections without querying the devices.
    const LEVEL_4: u32 = 4;
    /// The spooler's size answer is external data; past this cap nothing is read from it.
    const ENUMERATION_CAP: usize = 1024 * 1024;
    /// The same 1 MiB ceiling as ENUMERATION_CAP, counted in wide chars for a printer name.
    const DEFAULT_NAME_CAP: usize = ENUMERATION_CAP / 2;
    /// RAW hands the bytes to the printer untouched; its firmware renders the PDF.
    const RAW_DATATYPE: &str = "RAW";
    const DEFAULT_JOB_NAME: &str = "PDF document";

    /// Closes the spooler handle exactly once, even when a job fails midway.
    struct Spooler(PRINTER_HANDLE);

    impl Drop for Spooler {
        fn drop(&mut self) {
            // SAFETY: the handle came from OpenPrinterW below and closes exactly once, here.
            let _ = unsafe { ClosePrinter(self.0) };
        }
    }

    /// Sends `pdf` to `printer` (the default when `None`) and returns the id
    /// the spooler gave the last copy's job.
    pub(super) fn print(
        pdf: &[u8],
        printer: Option<&str>,
        job_name: Option<&str>,
        copies: u32,
    ) -> Result<u32, PdfExportError> {
        if pdf.is_empty() {
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
        let printer = match printer {
            Some(name) => name.to_string(),
            None => default_printer()?,
        };
        let job_name = job_name.unwrap_or(DEFAULT_JOB_NAME).to_string();

        let wide_name = HSTRING::from(printer.as_str());
        let mut handle = PRINTER_HANDLE::default();
        // SAFETY: writes only through the out-pointer we own.
        match unsafe { OpenPrinterW(&wide_name, &mut handle, None) } {
            Ok(()) => {}
            // Both codes mean no such printer: 123 for a malformed name, 1801
            // when the spooler rejects a well-formed but unknown name.
            Err(e)
                if e.code() == HRESULT::from_win32(ERROR_INVALID_NAME.0)
                    || e.code() == HRESULT::from_win32(ERROR_INVALID_PRINTER_NAME.0) =>
            {
                return Err(PdfExportError::new(ErrorKind::NoPrinter));
            }
            Err(e) => return Err(failed(e)),
        }
        let handle = Spooler(handle);

        // RAW passthrough has no copies field, so every copy is one spooled job.
        let mut job_id = 0;
        for _ in 0..copies {
            job_id = start_and_write(handle.0, &job_name, pdf)?;
        }
        Ok(job_id)
    }

    /// The printers the spooler knows, with the default marked. A machine
    /// with none is an empty list, not an error: callers list to display.
    pub(super) fn list() -> Result<Vec<Printer>, PdfExportError> {
        let default = default_printer().ok();
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let mut needed = 0u32;
        let mut returned = 0u32;
        // SAFETY: the probe passes no buffer, so the spooler only sizes; both out-pointers are ours.
        let probe = unsafe {
            EnumPrintersW(
                flags,
                PCWSTR::null(),
                LEVEL_4,
                None,
                &mut needed,
                &mut returned,
            )
        };
        if needed == 0 {
            // A probe with neither a size nor success is the spooler refusing to answer.
            return match probe {
                Ok(()) => Ok(Vec::new()),
                Err(e) => Err(backend(e)),
            };
        }
        let size = needed as usize;
        if size > ENUMERATION_CAP {
            return Err(PdfExportError::new(ErrorKind::Backend));
        }
        let mut buffer = vec![0u8; size];
        let mut returned = 0u32;
        // SAFETY: `buffer` holds the `needed` bytes the probe asked for; the spooler fills it.
        unsafe {
            EnumPrintersW(
                flags,
                PCWSTR::null(),
                LEVEL_4,
                Some(&mut buffer),
                &mut needed,
                &mut returned,
            )
        }
        .map_err(backend)?;

        let stride = std::mem::size_of::<PRINTER_INFO_4W>();
        let count = returned as usize;
        if count.saturating_mul(stride) > buffer.len() {
            return Err(PdfExportError::new(ErrorKind::Backend));
        }
        let mut printers = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: the offset stays inside the buffer, and read_unaligned
            // tolerates the byte buffer's weaker alignment.
            let info = unsafe {
                buffer
                    .as_ptr()
                    .add(index * stride)
                    .cast::<PRINTER_INFO_4W>()
                    .read_unaligned()
            };
            if info.pPrinterName.0.is_null() {
                continue;
            }
            // SAFETY: the spooler NUL-terminates the names it wrote into our buffer.
            let name = unsafe { info.pPrinterName.to_string() }.unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let is_default = default.as_deref() == Some(name.as_str());
            printers.push(Printer { name, is_default });
        }
        Ok(printers)
    }

    /// The user's default printer. Windows with no default set gets `NoPrinter`.
    fn default_printer() -> Result<String, PdfExportError> {
        let mut len = 0u32;
        // SAFETY: the probe passes no buffer, so only our out-pointer is touched.
        let probed = unsafe { GetDefaultPrinterW(None, &mut len) };
        // However the probe answered, leaving no size means no default was reported.
        if !probed.as_bool() && len == 0 {
            return Err(PdfExportError::new(ErrorKind::NoPrinter));
        }
        // A size answer past the cap is a misreport we refuse to allocate; this
        // fails the default-printer query as Backend, not the whole listing.
        if len as usize > DEFAULT_NAME_CAP {
            return Err(backend(WindowsError::from_thread()));
        }
        let mut buffer = vec![0u16; len as usize];
        // SAFETY: `buffer` holds the `len` wide characters the probe asked for.
        let filled =
            unsafe { GetDefaultPrinterW(Some(PWSTR::from_raw(buffer.as_mut_ptr())), &mut len) };
        if !filled.as_bool() {
            return Err(PdfExportError::new(ErrorKind::NoPrinter));
        }
        // The length the spooler reports is external data; the terminator is found, not assumed.
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let name = String::from_utf16_lossy(&buffer[..end]);
        if name.is_empty() {
            return Err(PdfExportError::new(ErrorKind::NoPrinter));
        }
        Ok(name)
    }

    /// Spools one job: start, all bytes, end. Ending runs even when the bytes
    /// fail, so the spooler is not left holding a half-written job.
    fn start_and_write(
        handle: PRINTER_HANDLE,
        job_name: &str,
        pdf: &[u8],
    ) -> Result<u32, PdfExportError> {
        let doc_name = HSTRING::from(job_name);
        let datatype = HSTRING::from(RAW_DATATYPE);
        let doc = DOC_INFO_1W {
            // The spooler only reads these strings; PWSTR is the struct's declared type.
            pDocName: PWSTR::from_raw(doc_name.as_ptr().cast_mut()),
            pOutputFile: PWSTR::null(),
            pDatatype: PWSTR::from_raw(datatype.as_ptr().cast_mut()),
        };
        // SAFETY: the handle is open, level 1 matches DOC_INFO_1W, and the
        // wide strings outlive the call.
        let job_id = unsafe { StartDocPrinterW(handle, 1, &doc) };
        if job_id == 0 {
            return Err(failed(WindowsError::from_thread()));
        }
        let written = write_all(handle, pdf);
        // SAFETY: the job started above, so it is ended whatever became of its bytes.
        let ended = unsafe { EndDocPrinter(handle) }.as_bool();
        match (written, ended) {
            (Ok(()), true) => Ok(job_id),
            (Err(e), _) => Err(e),
            (Ok(()), false) => Err(failed(WindowsError::from_thread())),
        }
    }

    /// Writes every byte, moving by what the spooler reports it took.
    fn write_all(handle: PRINTER_HANDLE, pdf: &[u8]) -> Result<(), PdfExportError> {
        let mut offset = 0usize;
        while offset < pdf.len() {
            let remaining = pdf.len() - offset;
            let len = remaining.min(u32::MAX as usize) as u32;
            let mut written = 0u32;
            // SAFETY: pointer and length describe the unwritten bytes; the
            // spooler reports how many it took through `written`.
            let ok =
                unsafe { WritePrinter(handle, pdf[offset..].as_ptr().cast(), len, &mut written) };
            if !ok.as_bool() {
                return Err(failed(WindowsError::from_thread()));
            }
            if written == 0 {
                // The spooler took nothing: stop instead of spinning on it.
                return Err(PdfExportError::new(ErrorKind::PrintFailed));
            }
            offset += written as usize;
        }
        Ok(())
    }

    /// A failed print carries the spooler's own code.
    fn failed(error: WindowsError) -> PdfExportError {
        PdfExportError::with_status(ErrorKind::PrintFailed, i64::from(error.code().0))
    }

    /// A failed listing carries the spooler's own code.
    fn backend(error: WindowsError) -> PdfExportError {
        PdfExportError::with_status(ErrorKind::Backend, i64::from(error.code().0))
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
        let backend = WindowsBackend::new();
        assert!(backend.supports(Capability::CreatePdf));
        assert!(backend.supports(Capability::SaveToPath));
        assert!(backend.supports(Capability::SaveDialog));
        assert!(!backend.supports(Capability::ConvertHtml));
        assert_eq!(
            backend.supports(Capability::Print),
            cfg!(target_os = "windows")
        );
        assert_eq!(
            backend.supports(Capability::ListPrinters),
            cfg!(target_os = "windows")
        );
    }

    #[tokio::test]
    async fn html_conversion_is_not_supported() {
        let error = WindowsBackend::new()
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
            let error = WindowsBackend::new()
                .print_pdf(b"%PDF-1.7", &print_options(Some(copies)))
                .await
                .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
            assert_eq!(error.code(), "ERR_PDF_EXPORT_INVALID_ARGUMENT");
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn print_and_listing_are_not_supported_off_windows() {
        let backend = WindowsBackend::new();
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
        WindowsBackend::new().cancel().await.unwrap();
    }
}
