//! [`PdfExport`]: the calls an app makes, one operation at a time, each bounded and resolved
//! exactly once. Also the engine under every bridge: text pages laid out through printpdf.

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use tokio::sync::oneshot;

use crate::{
    backend::{Backend, BackendFuture, Capability, PrintJob, Printer},
    error::{ErrorKind, PdfExportError},
    options::{ExportOptions, HtmlOptions, PageSize, PdfSource, PrintOptions, size_mm},
    platform,
};

/// The bound every operation resolves within, unless [`PdfExport::with_timeout`] moves it.
const DEFAULT_BOUND: Duration = Duration::from_secs(30);

/// The bound on one attempt to tell the backend to stop: the attempt must never hold the
/// caller's answer. Only targets with a timer run one.
#[cfg(not(target_arch = "wasm32"))]
const CANCEL_ATTEMPT_BOUND: Duration = Duration::from_millis(1000);

/// How the front reaches its backend: one it owns, or the process's own.
#[derive(Clone)]
enum BackendHolder {
    Owned(Arc<dyn Backend>),
    Static(&'static dyn Backend),
}

impl BackendHolder {
    fn backend(&self) -> &dyn Backend {
        match self {
            BackendHolder::Owned(backend) => backend.as_ref(),
            BackendHolder::Static(backend) => *backend,
        }
    }
}

/// The operation gate. `waiting` names the generation that owns it, so the release of an
/// operation that a cancel or the bound already settled cannot touch a newer call's sender.
#[derive(Default)]
struct Gate {
    next_id: u64,
    waiting: Option<(u64, oneshot::Sender<PdfExportError>)>,
}

struct Shared {
    backend: BackendHolder,
    gate: Mutex<Gate>,
    bound: Mutex<Duration>,
}

impl Shared {
    fn backend(&self) -> &dyn Backend {
        self.backend.backend()
    }

    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Frees the gate for `generation` only; a generation the gate no longer holds is done.
    fn release(&self, generation: u64) {
        let mut gate = self.gate();
        if matches!(&gate.waiting, Some((id, _)) if *id == generation) {
            gate.waiting = None;
        }
    }
}

/// Creating PDFs and printing them, on the platform's own terms.
///
/// It runs on [`platform::default_backend`] unless given another backend. One operation runs at
/// a time — a second while one runs resolves `ERR_PDF_EXPORT_IN_PROGRESS` — and every operation
/// resolves exactly once within its bound: a value, a coded error, or a cancel.
///
/// Each instance gates independently; clones share one gate. [`PdfExport::with_default_backend`]
/// shares the platform's backend, not a gate, so two instances gate separately.
#[derive(Clone)]
pub struct PdfExport {
    shared: Arc<Shared>,
}

impl PdfExport {
    /// The export through `backend`, with its own gate. Tests and fakes use this.
    pub fn new(backend: impl Backend + 'static) -> Self {
        Self {
            shared: Arc::new(Shared {
                backend: BackendHolder::Owned(Arc::new(backend)),
                gate: Mutex::new(Gate::default()),
                bound: Mutex::new(DEFAULT_BOUND),
            }),
        }
    }

    /// The platform's export: the backend is the process's own, the gate this instance's.
    pub fn with_default_backend() -> Self {
        Self {
            shared: Arc::new(Shared {
                backend: BackendHolder::Static(platform::default_backend()),
                gate: Mutex::new(Gate::default()),
                bound: Mutex::new(DEFAULT_BOUND),
            }),
        }
    }

    /// Moves the bound every operation resolves within. A bound of zero expires an operation
    /// that does not resolve at once.
    pub fn with_timeout(&mut self, bound: Duration) -> &mut Self {
        *self
            .shared
            .bound
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = bound;
        self
    }

    /// Whether the platform has `capability`.
    pub fn supports(&self, capability: Capability) -> bool {
        self.shared.backend().supports(capability)
    }

    /// Renders `source` to PDF bytes.
    pub async fn create_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
    ) -> Result<Vec<u8>, PdfExportError> {
        check_source(source, options)?;
        let backend = self.shared.backend();
        self.operate(backend.create_pdf(source, options)).await
    }

    /// Renders `source` and writes it to `path`. Resolves the path written.
    pub async fn save_pdf(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
        path: &Path,
    ) -> Result<PathBuf, PdfExportError> {
        check_source(source, options)?;
        let backend = self.shared.backend();
        self.operate(backend.save_pdf(source, options, path)).await
    }

    /// Renders `source` and writes it where the system's save dialog sends it. A dialog the
    /// user cancelled resolves `ERR_PDF_EXPORT_CANCELLED`. Resolves the path written.
    pub async fn save_pdf_with_dialog(
        &self,
        source: &PdfSource,
        options: &ExportOptions,
        suggested_name: &str,
    ) -> Result<PathBuf, PdfExportError> {
        check_source(source, options)?;
        let backend = self.shared.backend();
        let operation: BackendFuture<'_, Result<PathBuf, PdfExportError>> = Box::pin(async move {
            // `Ok(None)` is the user cancelling the dialog, not a failure.
            let Some(path) = backend.pick_save_path(suggested_name).await? else {
                return Err(PdfExportError::new(ErrorKind::Cancelled));
            };
            backend.save_pdf(source, options, &path).await
        });
        self.operate(operation).await
    }

    /// Renders `html` to PDF bytes.
    pub async fn convert_html(
        &self,
        html: &str,
        options: &HtmlOptions,
    ) -> Result<Vec<u8>, PdfExportError> {
        if html.trim().is_empty() {
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
        let backend = self.shared.backend();
        self.operate(backend.convert_html(html, options)).await
    }

    /// Sends `pdf` to a printer. Resolves the job the platform accepted.
    pub async fn print_pdf(
        &self,
        pdf: &[u8],
        options: &PrintOptions,
    ) -> Result<PrintJob, PdfExportError> {
        let copies = options.copies.unwrap_or(1);
        if !(1..=999).contains(&copies) {
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
        let backend = self.shared.backend();
        self.operate(backend.print_pdf(pdf, options)).await
    }

    /// The printers the system knows.
    pub async fn list_printers(&self) -> Result<Vec<Printer>, PdfExportError> {
        let backend = self.shared.backend();
        self.operate(backend.list_printers()).await
    }

    /// Cancels the running operation: it resolves `ERR_PDF_EXPORT_CANCELLED` and this resolves
    /// `Ok`. With nothing running it resolves `ERR_PDF_EXPORT_IN_PROGRESS`.
    pub async fn cancel(&self) -> Result<(), PdfExportError> {
        let waiting = {
            let mut gate = self.shared.gate();
            if gate.waiting.is_none() {
                return Err(PdfExportError::new(ErrorKind::InProgress));
            }
            gate.waiting.take()
        };
        if let Some((_, sender)) = waiting {
            let _ = sender.send(PdfExportError::new(ErrorKind::Cancelled));
        }
        // The cancel attempt must never hold the caller's answer, so it is bounded and
        // discarded: a backend that ignores cancel only delays cleanup, not resolution.
        #[cfg(not(target_arch = "wasm32"))]
        let _ = tokio::time::timeout(CANCEL_ATTEMPT_BOUND, self.shared.backend().cancel()).await;
        // wasm32 has no time driver to bound with; the web backend's cancel resolves at once,
        // so the attempt cannot hold the caller there either.
        #[cfg(target_arch = "wasm32")]
        let _ = self.shared.backend().cancel().await;
        Ok(())
    }

    /// Runs the only operation allowed at once. A second while one runs resolves
    /// `ERR_PDF_EXPORT_IN_PROGRESS`; the running one is unaffected. On the bound's expiry the
    /// call resolves `ERR_PDF_EXPORT_TIMEOUT`, a cancel is attempted on the backend, and the
    /// gate opens for the next call — the backend's late answer lands on a settled call.
    async fn operate<T>(
        &self,
        operation: impl Future<Output = Result<T, PdfExportError>>,
    ) -> Result<T, PdfExportError> {
        let (sender, mut receiver) = oneshot::channel();
        let generation = {
            let mut gate = self.shared.gate();
            if gate.waiting.is_some() {
                return Err(PdfExportError::new(ErrorKind::InProgress));
            }
            let generation = gate.next_id;
            gate.next_id += 1;
            gate.waiting = Some((generation, sender));
            generation
        };
        let mut operation = Box::pin(operation);
        // The timer needs tokio's time driver, which wasm32 has no runtime for; there the bound
        // is accepted and unused, as no operation of the web backend waits on the platform.
        //
        // Biassed, an operation that is ready resolves before the bound expires it, however
        // small the bound; a cancel settles before the bound takes the call.
        #[cfg(not(target_arch = "wasm32"))]
        let bound = *self
            .shared
            .bound
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        #[cfg(not(target_arch = "wasm32"))]
        let settled = tokio::select! {
            biased;
            answer = &mut operation => Outcome::Answer(answer),
            Ok(error) = &mut receiver => Outcome::Cancelled(error),
            _ = tokio::time::sleep(bound) => Outcome::Expired,
        };
        #[cfg(target_arch = "wasm32")]
        let settled = tokio::select! {
            biased;
            answer = &mut operation => Outcome::Answer(answer),
            Ok(error) = &mut receiver => Outcome::Cancelled(error),
        };
        match settled {
            Outcome::Answer(answer) => {
                self.shared.release(generation);
                answer
            }
            Outcome::Cancelled(error) => {
                self.shared.release(generation);
                Err(error)
            }
            #[cfg(not(target_arch = "wasm32"))]
            Outcome::Expired => {
                self.shared.release(generation);
                // A cancel is attempted, bounded: whether it lands no longer changes this call's
                // answer, and a backend that ignores it cannot hold the answer either.
                let _ = tokio::time::timeout(CANCEL_ATTEMPT_BOUND, self.shared.backend().cancel())
                    .await;
                Err(PdfExportError::new(ErrorKind::Timeout))
            }
        }
    }
}

impl Default for PdfExport {
    fn default() -> Self {
        Self::with_default_backend()
    }
}

/// How one operation ended: by its own answer, by a cancel, or by its bound.
enum Outcome<T> {
    Answer(Result<T, PdfExportError>),
    Cancelled(PdfExportError),
    /// The web has no timer to expire an operation with, so no operation ends this way there.
    #[cfg(not(target_arch = "wasm32"))]
    Expired,
}

/// v1 lays out text only, a document needs a page, and a page needs positive, finite sides.
fn check_source(source: &PdfSource, options: &ExportOptions) -> Result<(), PdfExportError> {
    if source.pages.is_empty() {
        return Err(PdfExportError::new(ErrorKind::InvalidArgument));
    }
    if let Some(PageSize::Custom {
        width_mm,
        height_mm,
    }) = options.page_size
    {
        let side = |mm: f64| mm.is_finite() && mm > 0.0;
        if !side(width_mm) || !side(height_mm) {
            return Err(PdfExportError::new(ErrorKind::InvalidArgument));
        }
    }
    Ok(())
}

/// The body text of a page, in points.
const FONT_SIZE_PT: f32 = 12.0;
/// The distance between two baselines, in points.
const LEADING_PT: f32 = 16.8;
/// The empty paper around every page's text, in millimetres.
const MARGIN_MM: f32 = 15.0;

/// Lays `source` out as PDF bytes through printpdf's ops-based API, the engine under every
/// bridge's `create_pdf` and `save_pdf`.
///
/// Font bytes are required: none is [`ErrorKind::FontNotFound`], bytes no font parser reads are
/// [`ErrorKind::FontInvalid`]. Nothing else about the source is checked here; [`PdfExport`]
/// validates before a backend is called.
pub(crate) async fn build_pdf(
    source: &PdfSource,
    options: &ExportOptions,
    font: Option<&[u8]>,
) -> Result<Vec<u8>, PdfExportError> {
    let font = font.ok_or_else(|| PdfExportError::new(ErrorKind::FontNotFound))?;
    let parsed = printpdf::ParsedFont::from_bytes(font, 0, &mut Vec::new())
        .ok_or_else(|| PdfExportError::new(ErrorKind::FontInvalid))?;
    let title = options.title.as_deref().unwrap_or(&source.title);
    let mut document = printpdf::PdfDocument::new(title);
    let font_id = document.add_font(&parsed);
    let page_size = options.page_size.unwrap_or(PageSize::A4);
    let size = size_mm(&page_size);
    let width = printpdf::Mm(size.width_mm as f32);
    let height = printpdf::Mm(size.height_mm as f32);
    // Baselines run down from the top margin, one leading apart.
    let left = printpdf::Pt::from(printpdf::Mm(MARGIN_MM));
    let first_baseline =
        printpdf::Pt::from(height).0 - printpdf::Pt::from(printpdf::Mm(MARGIN_MM)).0 - LEADING_PT;
    for page in &source.pages {
        let mut ops = vec![
            printpdf::Op::StartTextSection,
            printpdf::Op::SetFont {
                font: printpdf::PdfFontHandle::External(font_id.clone()),
                size: printpdf::Pt(FONT_SIZE_PT),
            },
        ];
        for (index, line) in page.lines.iter().enumerate() {
            ops.push(printpdf::Op::SetTextCursor {
                pos: printpdf::Point {
                    x: left,
                    y: printpdf::Pt(first_baseline - LEADING_PT * index as f32),
                },
            });
            ops.push(printpdf::Op::ShowText {
                items: vec![printpdf::TextItem::Text(line.clone())],
            });
        }
        ops.push(printpdf::Op::EndTextSection);
        document
            .pages
            .push(printpdf::PdfPage::new(width, height, ops));
    }
    let bytes = document.save(&printpdf::PdfSaveOptions::default(), &mut Vec::new());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::fake::FakeBackend;
    use crate::options::PageText;

    // The vendored font, copied from .refs/printpdf/examples/assets/fonts/RobotoMedium.ttf
    // (printpdf 0.12.8): the same bytes every platform bridge embeds. printpdf's own
    // defaultfonts/ subsets do not parse under `default-features = false`; this is the font
    // its customfont example embeds, and its glyphs render "Hello PDF".
    const TEST_FONT: &[u8] = include_bytes!("../assets/export-test-font.ttf");

    fn one_page() -> PdfSource {
        PdfSource::text(
            "Report",
            vec!["Hello PDF".to_owned(), "second line".to_owned()],
        )
    }

    fn parse(bytes: &[u8]) -> printpdf::PdfDocument {
        printpdf::PdfDocument::parse(
            bytes,
            &printpdf::PdfParseOptions::default(),
            &mut Vec::new(),
        )
        .expect("the document parses back")
    }

    #[tokio::test]
    async fn build_pdf_writes_hello_pdf_bytes() {
        let bytes = build_pdf(&one_page(), &ExportOptions::default(), Some(TEST_FONT))
            .await
            .expect("the engine builds");
        assert!(bytes.starts_with(b"%PDF-"), "the bytes must be a PDF");
        // Reading it back proves the text ops rendered, not just that bytes came out.
        let parsed = parse(&bytes);
        assert_eq!(parsed.page_count(), 1);
        let text = parsed.extract_text().concat().concat();
        assert!(
            text.contains("Hello PDF"),
            "the page must carry the text, got {text:?}"
        );
    }

    #[tokio::test]
    async fn build_pdf_lays_out_every_page() {
        let source = PdfSource {
            kind: Default::default(),
            title: "Two".to_owned(),
            pages: vec![
                PageText {
                    lines: vec!["one".to_owned()],
                },
                PageText {
                    lines: vec!["two".to_owned()],
                },
            ],
        };
        let bytes = build_pdf(&source, &ExportOptions::default(), Some(TEST_FONT))
            .await
            .expect("two pages build");
        assert_eq!(parse(&bytes).page_count(), 2);
    }

    #[tokio::test]
    async fn build_pdf_without_a_font_is_font_not_found() {
        let error = build_pdf(&one_page(), &ExportOptions::default(), None)
            .await
            .expect_err("font bytes are required");
        assert_eq!(error.kind(), ErrorKind::FontNotFound);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_FONT_NOT_FOUND");
    }

    #[tokio::test]
    async fn build_pdf_with_unreadable_bytes_is_font_invalid() {
        let error = build_pdf(&one_page(), &ExportOptions::default(), Some(b"not a font"))
            .await
            .expect_err("garbage bytes must not parse");
        assert_eq!(error.kind(), ErrorKind::FontInvalid);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_FONT_INVALID");
    }

    fn no_pages() -> PdfSource {
        PdfSource {
            kind: Default::default(),
            title: "Empty".to_owned(),
            pages: Vec::new(),
        }
    }

    #[tokio::test]
    async fn build_pdf_answers_even_with_no_pages() {
        // A bridge's save must reach its own write error, so the engine passes empty pages on.
        let bytes = build_pdf(&no_pages(), &ExportOptions::default(), Some(TEST_FONT))
            .await
            .expect("no pages is the front's argument to check, not the engine's");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[tokio::test]
    async fn an_empty_source_is_an_invalid_argument() {
        let export = PdfExport::new(FakeBackend::new(&Capability::ALL));
        let empty = no_pages();
        let error = export
            .create_pdf(&empty, &ExportOptions::default())
            .await
            .expect_err("a document needs a page");
        assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_INVALID_ARGUMENT");
    }

    #[tokio::test]
    async fn a_page_of_zero_or_infinite_sides_is_an_invalid_argument() {
        let export = PdfExport::new(FakeBackend::new(&Capability::ALL));
        for page_size in [
            PageSize::Custom {
                width_mm: 0.0,
                height_mm: 297.0,
            },
            PageSize::Custom {
                width_mm: 210.0,
                height_mm: -1.0,
            },
            PageSize::Custom {
                width_mm: f64::NAN,
                height_mm: 297.0,
            },
            PageSize::Custom {
                width_mm: f64::INFINITY,
                height_mm: 297.0,
            },
        ] {
            let options = ExportOptions {
                page_size: Some(page_size),
                title: None,
            };
            let error = export
                .create_pdf(&one_page(), &options)
                .await
                .expect_err("a page needs positive, finite sides");
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        }
    }

    #[tokio::test]
    async fn blank_html_is_an_invalid_argument() {
        let export = PdfExport::new(FakeBackend::new(&Capability::ALL));
        for html in ["", "   ", "\n\t "] {
            let error = export
                .convert_html(html, &HtmlOptions::default())
                .await
                .expect_err("blank html must not convert");
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        }
    }

    #[tokio::test]
    async fn copies_out_of_range_are_an_invalid_argument() {
        let export = PdfExport::new(FakeBackend::new(&Capability::ALL));
        for copies in [0, 1000] {
            let options = PrintOptions {
                printer_name: None,
                job_name: None,
                copies: Some(copies),
            };
            let error = export
                .print_pdf(b"%PDF-1.7", &options)
                .await
                .expect_err("copies must stay in 1..=999");
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        }
    }

    #[tokio::test]
    async fn an_invalid_argument_wins_over_the_one_operation_gate() {
        let export = PdfExport::new(FakeBackend::new(&Capability::ALL));
        let empty = no_pages();
        // Twice in a row: even with nothing else running, validation rejects before the gate.
        for _ in 0..2 {
            let error = export
                .create_pdf(&empty, &ExportOptions::default())
                .await
                .expect_err("validation first");
            assert_eq!(error.kind(), ErrorKind::InvalidArgument);
        }
    }

    #[tokio::test]
    async fn a_second_operation_is_in_progress_and_the_first_is_unaffected() {
        let fake = FakeBackend::new(&Capability::ALL);
        fake.set_delay(Duration::from_millis(200));
        let export = PdfExport::new(fake);
        let source = one_page();
        let options = ExportOptions::default();
        // `join!` polls in order: the first takes the gate and waits on its delay; the second
        // then finds the gate held.
        let (first, second) = tokio::join!(
            export.create_pdf(&source, &options),
            export.create_pdf(&source, &options)
        );
        let second = second.expect_err("one operation at a time");
        assert_eq!(second.kind(), ErrorKind::InProgress);
        assert_eq!(second.code(), "ERR_PDF_EXPORT_IN_PROGRESS");
        let bytes = first.expect("the first operation is unaffected");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[tokio::test]
    async fn cancel_settles_the_operation_and_then_reports_in_progress() {
        // A backend future is not `Send`, so the running call rides a local task.
        tokio::task::LocalSet::new()
            .run_until(async {
                let fake = FakeBackend::new(&Capability::ALL);
                fake.set_delay(Duration::from_secs(30));
                // Clones share one gate, so the handle that cancels is the task's own.
                let export = PdfExport::new(fake.clone());
                let in_task = export.clone();
                let source = one_page();
                let options = ExportOptions::default();
                let running =
                    tokio::task::spawn_local(
                        async move { in_task.create_pdf(&source, &options).await },
                    );
                // Wait until the backend holds the call, so the cancel races nothing.
                while !fake
                    .calls()
                    .iter()
                    .any(|call| matches!(call, crate::fake::FakeCall::CreatePdf { .. }))
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
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
                let none = export.cancel().await.expect_err("nothing runs any more");
                assert_eq!(none.kind(), ErrorKind::InProgress);
                assert!(
                    fake.calls()
                        .iter()
                        .any(|call| matches!(call, crate::fake::FakeCall::Cancel))
                );
            })
            .await;
    }

    #[tokio::test]
    async fn the_bound_expires_the_operation_and_the_gate_reopens() {
        let fake = FakeBackend::new(&Capability::ALL);
        fake.set_delay(Duration::from_secs(30));
        let mut export = PdfExport::new(fake.clone());
        export.with_timeout(Duration::from_millis(50));
        let started = Instant::now();
        let error = export
            .create_pdf(&one_page(), &ExportOptions::default())
            .await
            .expect_err("the bound expires the call");
        assert_eq!(error.kind(), ErrorKind::Timeout);
        assert_eq!(error.code(), "ERR_PDF_EXPORT_TIMEOUT");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the call resolves at its bound, not at the backend's"
        );
        assert!(
            fake.calls()
                .iter()
                .any(|call| matches!(call, crate::fake::FakeCall::Cancel)),
            "a cancel is attempted on the backend"
        );
        // The gate opened: the next operation starts, and the fake stays answerable.
        fake.set_delay(Duration::ZERO);
        let bytes = export
            .create_pdf(&one_page(), &ExportOptions::default())
            .await
            .expect("the next operation runs");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[tokio::test]
    async fn a_cancelled_dialog_resolves_cancelled_and_a_picked_path_saves() {
        let fake = FakeBackend::new(&Capability::ALL);
        fake.queue_pick(Ok(None));
        let export = PdfExport::new(fake.clone());
        let error = export
            .save_pdf_with_dialog(&one_page(), &ExportOptions::default(), "report.pdf")
            .await
            .expect_err("a cancelled dialog is a cancelled operation");
        assert_eq!(error.kind(), ErrorKind::Cancelled);

        fake.queue_pick(Ok(Some(PathBuf::from("/tmp/report.pdf"))));
        let saved = export
            .save_pdf_with_dialog(&one_page(), &ExportOptions::default(), "report.pdf")
            .await
            .expect("the picked path is written");
        assert_eq!(saved, PathBuf::from("/tmp/report.pdf"));
    }

    /// The fake, except `cancel` never settles: the front must still answer on its own.
    struct HangingCancel(FakeBackend);

    impl Backend for HangingCancel {
        fn supports(&self, capability: Capability) -> bool {
            self.0.supports(capability)
        }

        fn create_pdf(
            &self,
            source: &PdfSource,
            options: &ExportOptions,
        ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
            self.0.create_pdf(source, options)
        }

        fn save_pdf(
            &self,
            source: &PdfSource,
            options: &ExportOptions,
            path: &Path,
        ) -> BackendFuture<'_, Result<PathBuf, PdfExportError>> {
            self.0.save_pdf(source, options, path)
        }

        fn pick_save_path(
            &self,
            suggested_name: &str,
        ) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>> {
            self.0.pick_save_path(suggested_name)
        }

        fn convert_html(
            &self,
            html: &str,
            options: &HtmlOptions,
        ) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>> {
            self.0.convert_html(html, options)
        }

        fn print_pdf(
            &self,
            pdf: &[u8],
            options: &PrintOptions,
        ) -> BackendFuture<'_, Result<PrintJob, PdfExportError>> {
            self.0.print_pdf(pdf, options)
        }

        fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>> {
            self.0.list_printers()
        }

        fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>> {
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test]
    async fn a_hanging_backend_cancel_does_not_hold_the_cancel_answer() {
        // A backend future is not `Send`, so the running call rides a local task.
        tokio::task::LocalSet::new()
            .run_until(async {
                let fake = FakeBackend::new(&Capability::ALL);
                fake.set_delay(Duration::from_secs(30));
                let export = PdfExport::new(HangingCancel(fake.clone()));
                let in_task = export.clone();
                let source = one_page();
                let options = ExportOptions::default();
                let running =
                    tokio::task::spawn_local(
                        async move { in_task.create_pdf(&source, &options).await },
                    );
                while !fake
                    .calls()
                    .iter()
                    .any(|call| matches!(call, crate::fake::FakeCall::CreatePdf { .. }))
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                // The wrapper turns a regression into a failure here, not a hung run.
                tokio::time::timeout(Duration::from_secs(3), export.cancel())
                    .await
                    .expect("the cancel attempt must never hold the caller's answer")
                    .expect("a cancel that settled its operation resolves Ok");
                let cancelled = running
                    .await
                    .expect("the task joins")
                    .expect_err("the operation settles cancelled, not held");
                assert_eq!(cancelled.kind(), ErrorKind::Cancelled);
            })
            .await;
    }

    #[tokio::test]
    async fn a_hanging_backend_cancel_does_not_hold_the_timeout_answer() {
        let fake = FakeBackend::new(&Capability::ALL);
        fake.set_delay(Duration::from_secs(30));
        let mut export = PdfExport::new(HangingCancel(fake));
        export.with_timeout(Duration::from_millis(50));
        // The wrapper turns a regression into a failure here, not a hung run.
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            export.create_pdf(&one_page(), &ExportOptions::default()),
        )
        .await
        .expect("the cancel attempt must never hold the caller's answer")
        .expect_err("the bound expires the call");
        assert_eq!(error.kind(), ErrorKind::Timeout);
    }
}
