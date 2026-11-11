//! The front API's lifecycle rules through the fake backend: arguments validated before any
//! backend call, one operation at a time, each resolving exactly once — by its answer, a
//! cancel, or its bound.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nsh_pdf_export::{
    Capability, ErrorKind, ExportOptions, FakeBackend, FakeCall, HtmlOptions, PdfExport, PdfSource,
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

/// Yields until the fake has logged a create, so a second call or a cancel races nothing.
async fn settle_create(fake: &FakeBackend) {
    for _ in 0..2_000 {
        if fake
            .calls()
            .iter()
            .any(|call| matches!(call, FakeCall::CreatePdf { .. }))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the operation never reached the backend");
}

#[test]
fn convert_html_empty_html_is_invalid_argument() {
    let fake = FakeBackend::new(&Capability::ALL);
    let export = PdfExport::new(fake.clone());
    run(async {
        let error = export
            .convert_html("   \t\n", &HtmlOptions::default())
            .await
            .expect_err("blank html must not convert");
        assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_INVALID_ARGUMENT");
        assert!(fake.calls().is_empty(), "the backend is never asked");
    });
}

#[test]
fn create_pdf_empty_pages_is_invalid_argument() {
    let fake = FakeBackend::new(&Capability::ALL);
    let export = PdfExport::new(fake.clone());
    let source = PdfSource {
        kind: PdfSourceKind::Text,
        title: String::from("Empty"),
        pages: Vec::new(),
    };
    run(async {
        let error = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect_err("a document needs a page");
        assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        assert!(fake.calls().is_empty(), "the backend is never asked");
    });
}

#[test]
fn custom_page_size_zero_is_invalid_argument() {
    let fake = FakeBackend::new(&Capability::ALL);
    let export = PdfExport::new(fake.clone());
    let bad_sides = [
        nsh_pdf_export::PageSize::Custom {
            width_mm: 0.0,
            height_mm: 297.0,
        },
        nsh_pdf_export::PageSize::Custom {
            width_mm: 210.0,
            height_mm: -1.0,
        },
        nsh_pdf_export::PageSize::Custom {
            width_mm: f64::NAN,
            height_mm: 297.0,
        },
        nsh_pdf_export::PageSize::Custom {
            width_mm: f64::INFINITY,
            height_mm: 297.0,
        },
    ];
    run(async {
        for page_size in bad_sides {
            let options = ExportOptions {
                page_size: Some(page_size),
                title: None,
            };
            let error = export
                .create_pdf(&document(), &options)
                .await
                .expect_err("a page needs positive, finite sides");
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        }
        assert!(fake.calls().is_empty(), "the backend is never asked");
    });
}

#[test]
fn save_pdf_to_unwritable_path_is_write_failed() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_save(Err(nsh_pdf_export::PdfExportError::new(
        ErrorKind::WriteFailed,
    )));
    let export = PdfExport::new(fake.clone());
    run(async {
        let error = export
            .save_pdf(
                &document(),
                &ExportOptions::default(),
                Path::new("/no/such/directory/doc.pdf"),
            )
            .await
            .expect_err("a place the process cannot write reports itself");
        assert_eq!(error.kind(), ErrorKind::WriteFailed);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_WRITE_FAILED");
    });
}

#[test]
fn save_dialog_cancel_is_cancelled() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_pick(Ok(None));
    let export = PdfExport::new(fake.clone());
    run(async {
        let error = export
            .save_pdf_with_dialog(&document(), &ExportOptions::default(), "report.pdf")
            .await
            .expect_err("a cancelled dialog cancels the operation");
        assert_eq!(error.kind(), ErrorKind::Cancelled);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_CANCELLED");
        // The dialog was asked and nothing was written.
        assert_eq!(fake.calls().len(), 1);
        assert!(matches!(
            fake.calls()[0],
            FakeCall::PickSavePath { ref suggested_name } if suggested_name == "report.pdf"
        ));
    });
}

#[test]
fn second_concurrent_call_is_in_progress() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.set_delay(Duration::from_millis(200));
    let export = PdfExport::new(fake.clone());
    let source = document();
    let options = ExportOptions::default();
    run(async {
        // `join!` polls in order: the first takes the gate and waits on its delay; the second
        // then finds the gate held.
        let (first, second) = tokio::join!(
            export.create_pdf(&source, &options),
            export.create_pdf(&source, &options)
        );
        let error = second.expect_err("one operation at a time");
        assert_eq!(error.kind(), ErrorKind::InProgress);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_IN_PROGRESS");
        let bytes = first.expect("the first operation is unaffected");
        assert!(bytes.starts_with(b"%PDF-"));
    });
}

#[test]
fn cancel_cancels_active_op_exactly_once() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.set_delay(Duration::from_secs(30));
    let export = PdfExport::new(fake.clone());
    let in_task = export.clone();
    let source = document();
    let options = ExportOptions::default();
    // A backend future is not `Send`, so the running call rides a local task.
    run(async {
        tokio::task::LocalSet::new()
            .run_until(async move {
                let running =
                    tokio::task::spawn_local(
                        async move { in_task.create_pdf(&source, &options).await },
                    );
                settle_create(&fake).await;
                export
                    .cancel()
                    .await
                    .expect("a cancel lands on a running operation");
                let cancelled = running
                    .await
                    .expect("the task joins")
                    .expect_err("the operation resolves cancelled");
                assert_eq!(cancelled.kind(), ErrorKind::Cancelled);
                assert_eq!(cancelled.code(), "ERR_PDF_EXPORT_CANCELLED");
                // The gate is open again: a second cancel has nothing to settle.
                let idle = export.cancel().await.expect_err("nothing runs any more");
                assert_eq!(idle.kind(), ErrorKind::InProgress);
                assert_eq!(
                    fake.calls()
                        .iter()
                        .filter(|call| matches!(call, FakeCall::Cancel))
                        .count(),
                    1,
                    "the backend was told to stop once"
                );
            })
            .await
    });
}

#[test]
fn cancel_and_late_answer_resolves_once_cancelled() {
    // The scripted answer arrives after the cancel; the caller has already settled, once.
    let fake = FakeBackend::new(&Capability::ALL);
    fake.set_delay(Duration::from_millis(150));
    fake.queue_create(Ok(b"%PDF-late-answer".to_vec()));
    let export = PdfExport::new(fake.clone());
    let in_task = export.clone();
    let source = document();
    let task_source = source.clone();
    let options = ExportOptions::default();
    let task_options = options.clone();
    run(async {
        tokio::task::LocalSet::new()
            .run_until(async move {
                let running = tokio::task::spawn_local(async move {
                    in_task.create_pdf(&task_source, &task_options).await
                });
                settle_create(&fake).await;
                export.cancel().await.expect("the cancel lands");
                let cancelled = running
                    .await
                    .expect("the task joins")
                    .expect_err("the operation resolved cancelled");
                assert_eq!(cancelled.kind(), ErrorKind::Cancelled);
                // Let the late answer fire; it must add no call and disturb nothing.
                tokio::time::sleep(Duration::from_millis(400)).await;
                assert_eq!(fake.calls().len(), 2, "create and cancel, nothing more");
                let next = export
                    .create_pdf(&source, &options)
                    .await
                    .expect("the next operation runs on the reopened gate");
                assert!(next.starts_with(b"%PDF-"));
            })
            .await
    });
}

#[test]
fn bounded_wait_is_timeout_never_hangs() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.set_delay(Duration::from_secs(30));
    let mut export = PdfExport::new(fake.clone());
    export.with_timeout(Duration::from_millis(50));
    let source = document();
    run(async {
        let started = Instant::now();
        let error = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect_err("the bound expires the call");
        assert_eq!(error.kind(), ErrorKind::Timeout);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_TIMEOUT");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the call resolves at its bound, not at the backend's"
        );
        assert!(
            fake.calls()
                .iter()
                .any(|call| matches!(call, FakeCall::Cancel)),
            "a cancel is attempted on the backend"
        );
        // The gate opened: the next operation starts and the fake stays answerable.
        fake.set_delay(Duration::ZERO);
        let bytes = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect("the next operation runs");
        assert!(bytes.starts_with(b"%PDF-"));
    });
}

#[test]
fn print_with_no_printers_is_no_printer() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_printers(Ok(Vec::new()));
    fake.queue_print(Err(nsh_pdf_export::PdfExportError::new(
        ErrorKind::NoPrinter,
    )));
    let export = PdfExport::new(fake.clone());
    run(async {
        assert!(
            export
                .list_printers()
                .await
                .expect("the listing answers")
                .is_empty(),
            "no printers is an empty list"
        );
        let error = export
            .print_pdf(b"%PDF-1.7", &PrintOptions::default())
            .await
            .expect_err("printing with no printer reports itself");
        assert_eq!(error.kind(), ErrorKind::NoPrinter);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_NO_PRINTER");
    });
}

#[test]
fn print_to_unknown_printer_is_no_printer() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_print(Err(nsh_pdf_export::PdfExportError::new(
        ErrorKind::NoPrinter,
    )));
    let export = PdfExport::new(fake.clone());
    let options = PrintOptions {
        printer_name: Some(String::from("no such printer")),
        job_name: None,
        copies: None,
    };
    run(async {
        let error = export
            .print_pdf(b"%PDF-1.7", &options)
            .await
            .expect_err("a name no printer answers to reports itself");
        assert_eq!(error.kind(), ErrorKind::NoPrinter);
        assert!(
            !error.to_string().contains("no such printer"),
            "no printer name leaves in the message"
        );
    });
}

#[test]
fn in_progress_cleared_when_operation_settles() {
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_create(Err(nsh_pdf_export::PdfExportError::new(ErrorKind::Backend)));
    let export = PdfExport::new(fake.clone());
    let source = document();
    run(async {
        let first = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect_err("the scripted failure reaches the caller");
        assert_eq!(first.kind(), ErrorKind::Backend);
        // The failed operation released the gate: the next call starts, not InProgress.
        let bytes = export
            .create_pdf(&source, &ExportOptions::default())
            .await
            .expect("the next operation runs");
        assert!(bytes.starts_with(b"%PDF-"));
    });
}

#[test]
fn save_pdf_with_dialog_writes_the_picked_path() {
    // The dialog's happy leg: a picked path is written through, resolving that same path.
    let fake = FakeBackend::new(&Capability::ALL);
    fake.queue_pick(Ok(Some(PathBuf::from("/tmp/report.pdf"))));
    fake.queue_save(Ok(PathBuf::from("/tmp/report.pdf")));
    let export = PdfExport::new(fake.clone());
    run(async {
        let saved = export
            .save_pdf_with_dialog(&document(), &ExportOptions::default(), "report.pdf")
            .await
            .expect("the picked path is written");
        assert_eq!(saved, PathBuf::from("/tmp/report.pdf"));
        assert!(matches!(
            fake.calls()[1],
            FakeCall::SavePdf { ref path, .. } if path == Path::new("/tmp/report.pdf")
        ));
    });
}
