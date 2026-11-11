//! The seam between [`crate::PdfExport`] and a platform: what it can do, and the calls it answers.
//! Each call's future resolves exactly once — a value, a coded error, never a hang.

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

use crate::{
    error::PdfExportError,
    options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions},
};

/// A future a backend returns. Each call's future borrows the backend.
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// What a platform can do, as [`crate::PdfExport::supports`] reports it.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Rendering a source to PDF bytes.
    CreatePdf,
    /// Writing PDF bytes to an explicit path.
    SaveToPath,
    /// Showing the system's save dialog and writing where it sends.
    SaveDialog,
    /// Rendering HTML to PDF bytes.
    ConvertHtml,
    /// Sending PDF bytes to a printer.
    Print,
    /// Listing the printers the system knows.
    ListPrinters,
}

impl Capability {
    /// The name TS uses, such as `"createPdf"`.
    pub fn name(self) -> &'static str {
        match self {
            Capability::CreatePdf => "createPdf",
            Capability::SaveToPath => "saveToPath",
            Capability::SaveDialog => "saveDialog",
            Capability::ConvertHtml => "convertHtml",
            Capability::Print => "print",
            Capability::ListPrinters => "listPrinters",
        }
    }

    /// Every capability, in the order the platform matrix lists them.
    pub const ALL: [Capability; 6] = [
        Capability::CreatePdf,
        Capability::SaveToPath,
        Capability::SaveDialog,
        Capability::ConvertHtml,
        Capability::Print,
        Capability::ListPrinters,
    ];
}

/// A printer the system knows, as [`Backend::list_printers`] reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printer {
    /// The printer's name, as the print queue shows it.
    pub name: String,
    /// Whether the system prints here when no printer is named.
    pub is_default: bool,
}

/// A print job the system accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintJob {
    /// The id the platform gave the job, as a print queue knows it.
    pub id: i64,
}

/// What a platform provides. [`crate::PdfExport`] adds the rules: one operation at a time,
/// a bound each resolves within, and validation before any of it.
pub trait Backend: Send + Sync {
    /// Whether the platform has `capability`.
    fn supports(&self, capability: Capability) -> bool;

    /// Renders `source` to PDF bytes.
    fn create_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>>;

    /// Renders `source` and writes it to `path`. Resolves the path written.
    fn save_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
        path: &Path,
    ) -> BackendFuture<'_, Result<PathBuf, PdfExportError>>;

    /// Shows the system's save dialog. `Ok(None)` is the user cancelling, not a failure.
    fn pick_save_path(
        &self,
        suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>>;

    /// Renders `html` to PDF bytes.
    fn convert_html(
        &self,
        html: &str,
        options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>>;

    /// Sends `pdf` to a printer. Resolves the job the platform accepted.
    fn print_pdf(
        &self,
        pdf: &[u8],
        options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>>;

    /// The printers the system knows. A machine with none is an empty list, not an error.
    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>>;

    /// Asks the platform to stop the running operation. Where it cannot, this resolves
    /// anyway; the front API's bound is what resolves a stuck call.
    fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_has_the_name_ts_uses() {
        let names = [
            (Capability::CreatePdf, "createPdf"),
            (Capability::SaveToPath, "saveToPath"),
            (Capability::SaveDialog, "saveDialog"),
            (Capability::ConvertHtml, "convertHtml"),
            (Capability::Print, "print"),
            (Capability::ListPrinters, "listPrinters"),
        ];
        for (capability, name) in names {
            assert_eq!(capability.name(), name);
        }
        assert_eq!(Capability::ALL.len(), 6);
    }
}
