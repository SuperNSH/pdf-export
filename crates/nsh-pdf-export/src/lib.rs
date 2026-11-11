//! Creating PDFs from app content, and printing where the platform allows, for Native Shell
//! apps.
//!
//! [`PdfExport`] lays text out on paper, writes it where the app or the user sends it, and
//! prints where the platform has a spooler. Each call resolves exactly once, with a value, a
//! [`PdfExportError`] with a stable code, or a cancel — never a hang — and one operation runs
//! at a time per instance.

mod backend;
mod error;
mod export;
mod fake;
mod native;
mod options;
// Public so host tests construct each target's bridge directly, as the platform matrix has it.
pub mod platform;

use nsh_core::prelude::*;

pub use backend::{Backend, BackendFuture, Capability, PrintJob, Printer};
pub use error::{ErrorKind, PdfExportError};
pub use export::PdfExport;
pub use fake::{FakeBackend, FakeCall};
pub use native::{
    NativeExportOptions, NativeHtmlOptions, NativePageText, NativePdfExport, NativePdfSource,
    NativePrintJob, NativePrintOptions, NativePrinter,
};
pub use options::{
    ExportOptions, HtmlOptions, PageSize, PageText, PdfSource, PdfSourceKind, PrintOptions, SizeMm,
    size_mm, system_default,
};
pub use platform::default_backend;

/// The crate's UI set: what TSX imports from `nsh-pdf-export/native`.
#[native_module("nsh-pdf-export/native")]
fn native(registry: &mut Registry) {
    registry.class::<NativePdfExport>();
}
