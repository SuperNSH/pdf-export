//! The library's goals as runnable claims: the real engine renders a document, the v1
//! platform matrix holds on the bridges, and page sizes follow the locale.

use std::future::Future;

use nsh_pdf_export::{
    Backend, ExportOptions, HtmlOptions, PageSize, PageText, PdfExport, PdfSource, PrintOptions,
    default_backend, platform::android::AndroidBackend, platform::apple::AppleBackend,
    platform::web::WebBackend, size_mm, system_default,
};

fn run<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn pages(count: usize, lines: usize) -> Vec<PageText> {
    (0..count)
        .map(|_| PageText {
            lines: vec![String::from("a line of text"); lines],
        })
        .collect()
}

fn document(pages: Vec<PageText>) -> PdfSource {
    PdfSource {
        title: String::from("Report"),
        pages,
        ..PdfSource::default()
    }
}

#[test]
fn create_pdf_blank_page_is_valid_pdf() {
    let export = PdfExport::with_default_backend();
    let source = document(vec![PageText::default()]);
    run(async {
        let bytes = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect("a blank page is still a document");
        assert!(bytes.starts_with(b"%PDF-"));
    });
}

#[test]
fn create_pdf_large_document_resolves() {
    let export = PdfExport::with_default_backend();
    let source = document(pages(500, 40));
    run(async {
        let bytes = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect("500 pages resolve within the default bound");
        assert!(bytes.starts_with(b"%PDF-"));
    });
}

#[test]
fn web_print_is_not_available() {
    let backend = WebBackend::new();
    run(async {
        assert!(!backend.supports(nsh_pdf_export::Capability::Print));
        assert!(!backend.supports(nsh_pdf_export::Capability::ListPrinters));
        let print = backend
            .print_pdf(b"%PDF-1.7", &PrintOptions::default())
            .await
            .unwrap_err();
        assert_eq!(print.kind(), nsh_pdf_export::ErrorKind::NotAvailable);
        let printers = backend.list_printers().await.unwrap_err();
        assert_eq!(printers.kind(), nsh_pdf_export::ErrorKind::NotAvailable);
    });
}

#[test]
fn android_print_is_not_supported() {
    let backend = AndroidBackend::new();
    run(async {
        assert!(!backend.supports(nsh_pdf_export::Capability::Print));
        let error = backend
            .print_pdf(b"%PDF-1.7", &PrintOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), nsh_pdf_export::ErrorKind::NotSupported);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    });
}

#[test]
fn macos_print_is_not_supported_in_v1() {
    // The bridge answers the v1 matrix on any host, so the refusal is checked directly.
    let backend = AppleBackend::new();
    run(async {
        let error = backend
            .print_pdf(b"%PDF-1.7", &PrintOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), nsh_pdf_export::ErrorKind::NotSupported);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NOT_SUPPORTED");
    });
}

#[test]
fn convert_html_is_not_supported_in_v1() {
    run(async {
        // No native platform wires an HTML engine in v1; the web has none behind it at all.
        let native = default_backend()
            .convert_html("<p>Hello</p>", &HtmlOptions::default())
            .await
            .unwrap_err();
        assert_eq!(native.kind(), nsh_pdf_export::ErrorKind::NotSupported);
        let web = WebBackend::new()
            .convert_html("<p>Hello</p>", &HtmlOptions::default())
            .await
            .unwrap_err();
        assert_eq!(web.kind(), nsh_pdf_export::ErrorKind::NotAvailable);
    });
}

#[test]
fn page_size_system_default_respects_locale() {
    assert_eq!(system_default("en-US"), PageSize::Letter);
    for locale in ["en_US", "fr-FR", "ja-JP", ""] {
        assert_eq!(system_default(locale), PageSize::A4, "locale {locale:?}");
    }

    let table = [
        (PageSize::A3, 297.0, 420.0),
        (PageSize::A4, 210.0, 297.0),
        (PageSize::A5, 148.0, 210.0),
        (PageSize::Letter, 215.9, 279.4),
        (PageSize::Legal, 215.9, 355.6),
        (PageSize::Tabloid, 279.4, 431.8),
    ];
    for (page_size, width_mm, height_mm) in table {
        let size = size_mm(&page_size);
        assert_eq!(size.width_mm, width_mm, "the width of {page_size:?}");
        assert_eq!(size.height_mm, height_mm, "the height of {page_size:?}");
    }
    let us = size_mm(&system_default("en-US"));
    assert_eq!((us.width_mm, us.height_mm), (215.9, 279.4));
}
