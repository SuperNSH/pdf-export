//! A backend with scripted answers and a delay. Tests use it, and so can an app that runs
//! without a platform behind it. Every call is logged, in the order it came.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use crate::{
    backend::{Backend, BackendFuture, Capability, PrintJob, Printer},
    error::PdfExportError,
    options::{ExportOptions, HtmlOptions, PdfSource, PrintOptions},
};

/// The bytes the fake hands out as a PDF: the header of one, and nothing else.
const FAKE_PDF: &[u8] = b"%PDF-1.4";

/// One call the backend was asked, with what it was asked with.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FakeCall {
    /// A support query.
    Supports { capability: Capability },
    /// A render to bytes.
    CreatePdf {
        source: PdfSource,
        options: ExportOptions,
    },
    /// A render written to a path.
    SavePdf {
        source: PdfSource,
        options: ExportOptions,
        path: PathBuf,
    },
    /// The system's save dialog.
    PickSavePath { suggested_name: String },
    /// An HTML conversion.
    ConvertHtml { html: String, options: HtmlOptions },
    /// A print.
    PrintPdf { pdf: Vec<u8>, options: PrintOptions },
    /// A printer listing.
    ListPrinters,
    /// A cancel.
    Cancel,
}

struct FakeState {
    capabilities: Vec<Capability>,
    delay: Duration,
    create: VecDeque<Result<Vec<u8>, PdfExportError>>,
    save: VecDeque<Result<PathBuf, PdfExportError>>,
    pick: VecDeque<Result<Option<PathBuf>, PdfExportError>>,
    convert: VecDeque<Result<Vec<u8>, PdfExportError>>,
    print: VecDeque<Result<PrintJob, PdfExportError>>,
    printers: VecDeque<Result<Vec<Printer>, PdfExportError>>,
    log: Vec<FakeCall>,
    jobs: i64,
}

/// A backend whose answers the test scripts and whose every call it logs.
///
/// Each method answers from its own queue, or its default when the queue is empty, after the
/// delay. The queue entry is taken when the call arrives — as a timer fires however its caller
/// ends — so an answer is consumed exactly once, even by a caller that stopped waiting.
/// Clones share one state.
#[derive(Clone)]
pub struct FakeBackend {
    state: Arc<Mutex<FakeState>>,
}

impl FakeBackend {
    /// A backend answering `capabilities`, with no delay and every queue empty.
    pub fn new(capabilities: &[Capability]) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                capabilities: capabilities.to_vec(),
                delay: Duration::ZERO,
                create: VecDeque::new(),
                save: VecDeque::new(),
                pick: VecDeque::new(),
                convert: VecDeque::new(),
                print: VecDeque::new(),
                printers: VecDeque::new(),
                log: Vec::new(),
                jobs: 0,
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets how long every answer waits before it resolves.
    pub fn set_delay(&self, delay: Duration) {
        self.lock().delay = delay;
    }

    /// Scripts the next `create_pdf` answer.
    pub fn queue_create(&self, answer: Result<Vec<u8>, PdfExportError>) {
        self.lock().create.push_back(answer);
    }

    /// Scripts the next `save_pdf` answer.
    pub fn queue_save(&self, answer: Result<PathBuf, PdfExportError>) {
        self.lock().save.push_back(answer);
    }

    /// Scripts the next `pick_save_path` answer.
    pub fn queue_pick(&self, answer: Result<Option<PathBuf>, PdfExportError>) {
        self.lock().pick.push_back(answer);
    }

    /// Scripts the next `convert_html` answer.
    pub fn queue_convert(&self, answer: Result<Vec<u8>, PdfExportError>) {
        self.lock().convert.push_back(answer);
    }

    /// Scripts the next `print_pdf` answer.
    pub fn queue_print(&self, answer: Result<PrintJob, PdfExportError>) {
        self.lock().print.push_back(answer);
    }

    /// Scripts the next `list_printers` answer.
    pub fn queue_printers(&self, answer: Result<Vec<Printer>, PdfExportError>) {
        self.lock().printers.push_back(answer);
    }

    /// Every call so far, in the order they came.
    pub fn calls(&self) -> Vec<FakeCall> {
        self.lock().log.clone()
    }

    /// Takes the call's scripted answer, or `None` to answer the default.
    fn scripted<T>(
        queue: &mut VecDeque<Result<T, PdfExportError>>,
    ) -> Option<Result<T, PdfExportError>> {
        queue.pop_front()
    }
}

/// Waits `delay`, where a clock exists; wasm32 has no runtime to drive a timer, so there it
/// resolves at once — the delay is only simulated on hosts.
async fn after_delay(delay: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(delay).await;
    #[cfg(target_arch = "wasm32")]
    let _ = delay;
}

impl Backend for FakeBackend {
    fn supports(&self, capability: Capability) -> bool {
        self.lock().log.push(FakeCall::Supports { capability });
        self.lock().capabilities.contains(&capability)
    }

    fn create_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        let (delay, answer) = {
            let mut state = self.lock();
            state.log.push(FakeCall::CreatePdf {
                source: source.clone(),
                options: options.clone(),
            });
            (
                state.delay,
                Self::scripted(&mut state.create).unwrap_or_else(|| Ok(FAKE_PDF.to_vec())),
            )
        };
        Box::pin(async move {
            after_delay(delay).await;
            answer
        })
    }

    fn save_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
        path: &std::path::Path,
    ) -> BackendFuture<'_, Result<PathBuf, PdfExportError>> {
        let (delay, answer) = {
            let mut state = self.lock();
            state.log.push(FakeCall::SavePdf {
                source: source.clone(),
                options: options.clone(),
                path: path.to_path_buf(),
            });
            (
                state.delay,
                Self::scripted(&mut state.save).unwrap_or_else(|| Ok(path.to_path_buf())),
            )
        };
        Box::pin(async move {
            after_delay(delay).await;
            answer
        })
    }

    fn pick_save_path(
        &self,
        suggested_name: &str,
    ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
        let (delay, answer) = {
            let mut state = self.lock();
            state.log.push(FakeCall::PickSavePath {
                suggested_name: suggested_name.to_owned(),
            });
            (
                state.delay,
                Self::scripted(&mut state.pick)
                    .unwrap_or_else(|| Ok(Some(PathBuf::from(suggested_name)))),
            )
        };
        Box::pin(async move {
            after_delay(delay).await;
            answer
        })
    }

    fn convert_html(
        &self,
        html: &str,
        options: &HtmlOptions,
    ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
        let (delay, answer) = {
            let mut state = self.lock();
            state.log.push(FakeCall::ConvertHtml {
                html: html.to_owned(),
                options: options.clone(),
            });
            (
                state.delay,
                Self::scripted(&mut state.convert).unwrap_or_else(|| Ok(FAKE_PDF.to_vec())),
            )
        };
        Box::pin(async move {
            after_delay(delay).await;
            answer
        })
    }

    fn print_pdf(
        &self,
        pdf: &[u8],
        options: &PrintOptions,
    ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
        let (delay, answer) = {
            let mut state = self.lock();
            state.log.push(FakeCall::PrintPdf {
                pdf: pdf.to_vec(),
                options: options.clone(),
            });
            state.jobs += 1;
            let job = PrintJob { id: state.jobs };
            (
                state.delay,
                Self::scripted(&mut state.print).unwrap_or(Ok(job)),
            )
        };
        Box::pin(async move {
            after_delay(delay).await;
            answer
        })
    }

    fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
        let (delay, answer) = {
            let mut state = self.lock();
            state.log.push(FakeCall::ListPrinters);
            (
                state.delay,
                Self::scripted(&mut state.printers).unwrap_or_else(|| {
                    Ok(vec![Printer {
                        name: "Memory Printer".to_owned(),
                        is_default: true,
                    }])
                }),
            )
        };
        Box::pin(async move {
            after_delay(delay).await;
            answer
        })
    }

    fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>> {
        self.lock().log.push(FakeCall::Cancel);
        Box::pin(std::future::ready(Ok(())))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn supports_answers_the_list_it_was_given_and_logs_it() {
        let fake = FakeBackend::new(&[Capability::CreatePdf, Capability::Print]);
        assert!(fake.supports(Capability::CreatePdf));
        assert!(!fake.supports(Capability::SaveDialog));
        assert_eq!(
            fake.calls(),
            vec![
                FakeCall::Supports {
                    capability: Capability::CreatePdf
                },
                FakeCall::Supports {
                    capability: Capability::SaveDialog
                },
            ]
        );
    }

    #[tokio::test]
    async fn empty_queues_answer_the_defaults() {
        let fake = FakeBackend::new(&Capability::ALL);
        let source = PdfSource::text("Report", vec!["Hello PDF".to_owned()]);
        assert!(
            fake.create_pdf(&source, &ExportOptions::default())
                .await
                .unwrap()
                .starts_with(b"%PDF-")
        );
        assert_eq!(
            fake.save_pdf(
                &source,
                &ExportOptions::default(),
                Path::new("/tmp/out.pdf")
            )
            .await
            .unwrap(),
            PathBuf::from("/tmp/out.pdf")
        );
        assert_eq!(
            fake.pick_save_path("doc.pdf").await.unwrap(),
            Some(PathBuf::from("doc.pdf"))
        );
        assert_eq!(
            fake.print_pdf(b"%PDF-1.7", &PrintOptions::default())
                .await
                .unwrap(),
            PrintJob { id: 1 }
        );
        assert_eq!(
            fake.list_printers().await.unwrap(),
            vec![Printer {
                name: "Memory Printer".to_owned(),
                is_default: true
            }]
        );
    }

    #[tokio::test]
    async fn scripted_answers_shift_their_queues() {
        let fake = FakeBackend::new(&Capability::ALL);
        fake.queue_create(Err(PdfExportError::new(ErrorKind::Backend)));
        fake.queue_create(Ok(vec![1, 2, 3]));
        let source = PdfSource::text("Report", vec!["line".to_owned()]);
        assert_eq!(
            fake.create_pdf(&source, &ExportOptions::default())
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::Backend
        );
        assert_eq!(
            fake.create_pdf(&source, &ExportOptions::default())
                .await
                .unwrap(),
            vec![1, 2, 3]
        );
        // The queue is empty again: the default answers.
        assert!(
            fake.create_pdf(&source, &ExportOptions::default())
                .await
                .unwrap()
                .starts_with(b"%PDF-")
        );
    }

    #[tokio::test]
    async fn a_delayed_answer_still_comes_once_after_its_caller_gave_up() {
        let fake = FakeBackend::new(&Capability::ALL);
        fake.set_delay(Duration::from_secs(30));
        fake.queue_create(Ok(vec![9, 9]));
        let source = PdfSource::text("Report", vec!["line".to_owned()]);
        let given_up = fake.create_pdf(&source, &ExportOptions::default());
        // The answer left its queue with the call; dropping the future abandons only the wait.
        drop(given_up);
        fake.set_delay(Duration::ZERO);
        assert_eq!(
            fake.create_pdf(&source, &ExportOptions::default())
                .await
                .unwrap(),
            FAKE_PDF.to_vec()
        );
    }

    #[tokio::test]
    async fn every_call_is_logged_with_what_it_was_asked() {
        let fake = FakeBackend::new(&Capability::ALL);
        let source = PdfSource::text("Report", vec!["Hello PDF".to_owned()]);
        let options = ExportOptions {
            page_size: Some(crate::options::PageSize::A4),
            title: Some("Report".to_owned()),
        };
        let _ = fake.create_pdf(&source, &options).await;
        let _ = fake
            .convert_html("<p>Hello PDF</p>", &HtmlOptions::default())
            .await;
        let _ = fake.cancel().await;
        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0],
            FakeCall::CreatePdf {
                source: source.clone(),
                options: options.clone()
            }
        );
        assert_eq!(
            calls[1],
            FakeCall::ConvertHtml {
                html: "<p>Hello PDF</p>".to_owned(),
                options: HtmlOptions::default()
            }
        );
        assert_eq!(calls[2], FakeCall::Cancel);
    }
}
