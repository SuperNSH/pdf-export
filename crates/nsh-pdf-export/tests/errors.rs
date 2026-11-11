//! The error surface as callers see it: every kind keeps its code and message, the font codes
//! reach an app, and no message carries the caller's own data.

use std::time::Duration;

use nsh_pdf_export::{
    Capability, ErrorKind, ExportOptions, FakeBackend, PdfExport, PdfExportError, PdfSource,
    PdfSourceKind, PrintOptions,
};

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn document() -> PdfSource {
    PdfSource {
        kind: PdfSourceKind::Text,
        title: String::from("Report"),
        pages: vec![nsh_pdf_export::PageText {
            lines: vec![String::from("a line of text")],
        }],
    }
}

// The contract's table: every kind carries this code and this message, word for word.
const ALL_KINDS: [(ErrorKind, &str, &str); 12] = [
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

#[test]
fn every_kind_keeps_its_code_its_message_and_no_status() {
    for (kind, code, message) in ALL_KINDS {
        let error = PdfExportError::new(kind);
        assert_eq!(error.kind(), kind);
        assert_eq!(error.code(), code);
        assert_eq!(error.to_string(), message, "the message of {code}");
        assert_eq!(error.status(), None);
    }
    let statusful = PdfExportError::with_status(ErrorKind::PrintFailed, 1032);
    assert_eq!(statusful.status(), Some(1032));
    assert_eq!(statusful.to_string(), "The print job failed.");
}

// The engine (`build_pdf`) is crate-private, so the font codes are checked here the way an app
// meets them: delivered through the front API by the platform backend.
#[test]
fn create_pdf_without_font_is_font_not_found() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_create(Err(PdfExportError::new(ErrorKind::FontNotFound)));
    let export = PdfExport::new(fake);
    run(async {
        let error = export
            .create_pdf(&document(), &ExportOptions::default())
            .await
            .expect_err("no font bytes is a coded failure");
        assert_eq!(error.kind(), ErrorKind::FontNotFound);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_FONT_NOT_FOUND");
        assert_eq!(error.to_string(), "The requested font is not available.");
    });
}

#[test]
fn create_pdf_with_bad_font_bytes_is_font_invalid() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_create(Err(PdfExportError::new(ErrorKind::FontInvalid)));
    let export = PdfExport::new(fake);
    run(async {
        let error = export
            .create_pdf(&document(), &ExportOptions::default())
            .await
            .expect_err("unreadable font bytes are a coded failure");
        assert_eq!(error.kind(), ErrorKind::FontInvalid);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_FONT_INVALID");
        assert_eq!(error.to_string(), "The font bytes could not be read.");
    });
}

#[test]
fn error_message_holds_no_user_data() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_convert(Err(PdfExportError::new(ErrorKind::Backend)));
    fake.queue_save(Err(PdfExportError::new(ErrorKind::WriteFailed)));
    fake.queue_print(Err(PdfExportError::new(ErrorKind::PrintFailed)));
    let export = PdfExport::new(fake);
    let secrets = [
        "SECRET-HTML-7f31",
        "/no/such/SECRET-PATH-8e22/report.pdf",
        "SECRET-PRINTER-9d43",
    ];
    run(async {
        let errors = [
            export
                .convert_html("<p>SECRET-HTML-7f31</p>", &Default::default())
                .await
                .unwrap_err(),
            export
                .save_pdf(
                    &document(),
                    &ExportOptions::default(),
                    std::path::Path::new("/no/such/SECRET-PATH-8e22/report.pdf"),
                )
                .await
                .unwrap_err(),
            export
                .print_pdf(
                    b"%PDF-1.7",
                    &PrintOptions {
                        printer_name: Some(String::from("SECRET-PRINTER-9d43")),
                        job_name: None,
                        copies: None,
                    },
                )
                .await
                .unwrap_err(),
        ];
        for error in errors {
            for secret in secrets {
                assert!(
                    !error.to_string().contains(secret),
                    "the message must not carry {secret}: {error}"
                );
            }
        }
    });
}

#[test]
fn a_bound_of_zero_expires_an_operation_that_does_not_resolve_at_once() {
    // The bound's floor: even zero resolves the call rather than leaving it hanging.
    let fake = FakeBackend::new(&Capability::ALL);
    fake.set_delay(Duration::from_secs(30));
    let mut export = PdfExport::new(fake);
    export.with_timeout(Duration::ZERO);
    run(async {
        let error = export
            .create_pdf(&document(), &ExportOptions::default())
            .await
            .expect_err("a zero bound still resolves the call");
        assert_eq!(error.kind(), ErrorKind::Timeout);
    });
}
