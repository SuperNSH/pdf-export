//! The errors an export or print reports. Each has a stable code; a message says what to do.

use std::fmt;

/// What went wrong, as a stable code; see [`PdfExportError::code`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// No PDF export service exists on this platform (the web).
    NotAvailable,
    /// The platform has the service but not this capability, in v1.
    NotSupported,
    /// An argument is empty or not allowed, such as blank html or a page side of zero.
    InvalidArgument,
    /// The user, the app or the front API cancelled the operation.
    Cancelled,
    /// The call gave up waiting: its bound passed with the backend still working.
    Timeout,
    /// Another operation is already running. One operation runs at a time.
    InProgress,
    /// No font bytes were given, and the platform has none to substitute.
    FontNotFound,
    /// The font bytes given could not be read as a font.
    FontInvalid,
    /// The file could not be written to its path.
    WriteFailed,
    /// No printer is available on the device.
    NoPrinter,
    /// The print job failed; `status` holds the platform's own code where it gives one.
    PrintFailed,
    /// The platform's PDF export service failed; `status` holds its own code where it gives one.
    Backend,
}

/// A failed export, save, conversion, print or printer listing.
///
/// `Display` names the code and the message. Neither holds source text, paths or printer names.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", .kind.message())]
pub struct PdfExportError {
    kind: ErrorKind,
    status: Option<i64>,
}

impl PdfExportError {
    /// An error of `kind`, with no platform status.
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, status: None }
    }

    /// An error of `kind`, carrying the platform's own status code.
    #[must_use]
    pub fn with_status(kind: ErrorKind, status: i64) -> Self {
        Self {
            kind,
            status: Some(status),
        }
    }

    /// What went wrong.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The platform's status code, when the platform gave one.
    pub fn status(&self) -> Option<i64> {
        self.status
    }

    /// The stable code, such as `ERR_PDF_EXPORT_CANCELLED`. Apps match on this.
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl ErrorKind {
    /// The stable code of this kind.
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::NotAvailable => "ERR_PDF_EXPORT_NOT_AVAILABLE",
            ErrorKind::NotSupported => "ERR_PDF_EXPORT_NOT_SUPPORTED",
            ErrorKind::InvalidArgument => "ERR_PDF_EXPORT_INVALID_ARGUMENT",
            ErrorKind::Cancelled => "ERR_PDF_EXPORT_CANCELLED",
            ErrorKind::Timeout => "ERR_PDF_EXPORT_TIMEOUT",
            ErrorKind::InProgress => "ERR_PDF_EXPORT_IN_PROGRESS",
            ErrorKind::FontNotFound => "ERR_PDF_EXPORT_FONT_NOT_FOUND",
            ErrorKind::FontInvalid => "ERR_PDF_EXPORT_FONT_INVALID",
            ErrorKind::WriteFailed => "ERR_PDF_EXPORT_WRITE_FAILED",
            ErrorKind::NoPrinter => "ERR_PDF_EXPORT_NO_PRINTER",
            ErrorKind::PrintFailed => "ERR_PDF_EXPORT_PRINT_FAILED",
            ErrorKind::Backend => "ERR_PDF_EXPORT_BACKEND",
        }
    }

    /// What to do about it, in one sentence.
    pub fn message(self) -> &'static str {
        match self {
            ErrorKind::NotAvailable => "PDF export is not available on this platform.",
            ErrorKind::NotSupported => "This capability is not supported here yet.",
            ErrorKind::InvalidArgument => "An argument is empty or not allowed on this platform.",
            ErrorKind::Cancelled => "The operation was cancelled.",
            ErrorKind::Timeout => "The operation timed out. Try again.",
            ErrorKind::InProgress => {
                "Another export or print is already running. Wait for it to finish."
            }
            ErrorKind::FontNotFound => "The requested font is not available.",
            ErrorKind::FontInvalid => "The font bytes could not be read.",
            ErrorKind::WriteFailed => "The file could not be written.",
            ErrorKind::NoPrinter => "No printer is available.",
            ErrorKind::PrintFailed => "The print job failed.",
            ErrorKind::Backend => "The platform's PDF export service failed.",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_its_code_and_its_message() {
        let pairs = [
            (
                ErrorKind::NotAvailable,
                "ERR_PDF_EXPORT_NOT_AVAILABLE",
                "PDF export is not available on this platform.",
            ),
            (
                ErrorKind::NotSupported,
                "ERR_PDF_EXPORT_NOT_SUPPORTED",
                "This capability is not supported here yet.",
            ),
            (
                ErrorKind::InvalidArgument,
                "ERR_PDF_EXPORT_INVALID_ARGUMENT",
                "An argument is empty or not allowed on this platform.",
            ),
            (
                ErrorKind::Cancelled,
                "ERR_PDF_EXPORT_CANCELLED",
                "The operation was cancelled.",
            ),
            (
                ErrorKind::Timeout,
                "ERR_PDF_EXPORT_TIMEOUT",
                "The operation timed out. Try again.",
            ),
            (
                ErrorKind::InProgress,
                "ERR_PDF_EXPORT_IN_PROGRESS",
                "Another export or print is already running. Wait for it to finish.",
            ),
            (
                ErrorKind::FontNotFound,
                "ERR_PDF_EXPORT_FONT_NOT_FOUND",
                "The requested font is not available.",
            ),
            (
                ErrorKind::FontInvalid,
                "ERR_PDF_EXPORT_FONT_INVALID",
                "The font bytes could not be read.",
            ),
            (
                ErrorKind::WriteFailed,
                "ERR_PDF_EXPORT_WRITE_FAILED",
                "The file could not be written.",
            ),
            (
                ErrorKind::NoPrinter,
                "ERR_PDF_EXPORT_NO_PRINTER",
                "No printer is available.",
            ),
            (
                ErrorKind::PrintFailed,
                "ERR_PDF_EXPORT_PRINT_FAILED",
                "The print job failed.",
            ),
            (
                ErrorKind::Backend,
                "ERR_PDF_EXPORT_BACKEND",
                "The platform's PDF export service failed.",
            ),
        ];
        for (kind, code, message) in pairs {
            assert_eq!(kind.code(), code, "the code of {kind:?}");
            assert_eq!(kind.message(), message, "the message of {code}");
            assert_eq!(PdfExportError::new(kind).to_string(), message);
            assert_eq!(PdfExportError::new(kind).code(), code);
            assert_eq!(PdfExportError::new(kind).kind(), kind);
            assert_eq!(PdfExportError::new(kind).status(), None);
        }
    }

    #[test]
    fn a_status_rides_along_and_the_display_stays_the_message() {
        let error = PdfExportError::with_status(ErrorKind::PrintFailed, 1032);
        assert_eq!(error.status(), Some(1032));
        assert_eq!(error.kind(), ErrorKind::PrintFailed);
        assert_eq!(error.to_string(), "The print job failed.");
    }

    #[test]
    fn the_kinds_display_as_their_codes() {
        assert_eq!(ErrorKind::Cancelled.to_string(), "ERR_PDF_EXPORT_CANCELLED");
    }
}
