// nsh-pdf-export build-out: ten areas advancing alone, in parallel.
// Each area implements, then runs its own audit loop (3 rounds: audit, fix,
// until zero issues or the cap) with a scoped check; the full checks run once
// at the join. Every agent is pinned to GLM-5.3 per the brief. No git
// machinery anywhere: areas edit the current checkout directly.

interface StepResult {
  /** True when the area's own files are written and its own checks pass. */
  done: boolean;
  /** What was built, in two or three sentences. */
  summary: string;
  /** Anything genuinely blocked, with the reason. */
  blockers?: string[];
  /** One sentence copied verbatim from the contract text that most shaped the work. */
  contractLine: string;
}

interface Finding {
  /** Workspace-relative path, with a line when it applies: "src/a.rs:42". */
  where: string;
  /** One sentence: what is wrong. */
  what: string;
  /** What showed it: the lines read, or the command output that proved it. */
  evidence: string;
  /** How much it matters. Reserve "high" for a hang, a panic, or a wrong result. */
  severity: "low" | "medium" | "high";
}

interface AuditResult {
  /** Actionable findings in THIS area's own files only. Empty means clean. */
  findings: Finding[];
  /** Failures that come only from files another area owns, still in flight. Notes, not findings. */
  external: string[];
  /** One sentence copied verbatim from the contract text that most shaped the review. */
  contractLine: string;
}

interface AreaOutcome {
  name: string;
  /** "done" first-pass clean, "converged" after fix rounds, "cap" with findings left, "failed". */
  status: "done" | "converged" | "cap" | "failed";
  summary: string;
  rounds: number;
  personaDelivered: "verified" | "unverified";
}

interface GateResult {
  name: string;
  ok: boolean;
  exitCode: number;
  tail: string;
}

type AskResult<T> = { value: T; delivered: boolean };

const MODELS = {
  pro: model("account:zai-individual-coding-plan/GLM-5.3$high"),
};

const AUDIT_ROUNDS = 3;
const KIT = "/Users/hmziq/ZCodeProject/ai/harness/zcode/agents";

async function specialist(name: string, fallback: string): Promise<{ persona: string; source: string }> {
  const adapter =
    `You are the ${name} specialist for one area of a parallel workflow. ` +
    `Your full specialist contract follows verbatim — apply it to this area's work; ` +
    `where it conflicts with your ask, the ask wins.` + "\n\n";
  for (const p of [`.zcode/agents/${name}.md`, `${KIT}/${name}.md`]) {
    try {
      const raw = await files.read(p);
      return { persona: adapter + raw.replace(/^---\n[\s\S]*?\n---\n/, "").trim(), source: p };
    } catch { /* try next source */ }
  }
  return { persona: fallback, source: "fallback" };
}

function tail(text: string, max = 3500): string {
  const clean = (text ?? "").trim();
  return clean.length <= max ? clean : "…(earlier output cut)…" + clean.slice(clean.length - max);
}

async function askedWithContract<T extends { contractLine: string }>(
  ag: ReturnType<typeof agent>,
  persona: string,
  askText: string,
): Promise<AskResult<T>> {
  const first = await ag.ask<T>(askText);
  const line = typeof first.contractLine === "string" ? first.contractLine.trim() : "";
  if (line.length > 0 && persona.includes(line)) return { value: first, delivered: true };
  const retry = await ag.ask<T>(
    "Your contractLine must be one sentence copied VERBATIM from the contract text you were given " +
    "(your instructions), not a paraphrase and not your own words. Copy it exactly and return the result again.",
  );
  const retryLine = typeof retry.contractLine === "string" ? retry.contractLine.trim() : "";
  return { value: retry, delivered: retryLine.length > 0 && persona.includes(retryLine) };
}

// The one contract every area implements. Fixed before the run so the areas,
// which advance at the same time, cannot drift apart.
const CONTRACT = [
  "PROJECT CONTRACT — nsh-pdf-export (Native Shell PDF export and printing).",
  "",
  "Repo: /Users/hmziq/fo/nsh-pdf-export (work there; the checkout is already on master, clean).",
  "Layout mirrors SuperNSH/biometrics (read .refs/biometrics for the conventions):",
  "  crates/nsh-pdf-export  — the Rust crate (workspace member; the root Cargo.toml becomes a [workspace] manifest)",
  "  packages/nsh-pdf-export — the TSX package (package.json, tsconfig.json, nsh.json, App.tsx, src/, tests/)",
  "Local reference clones (READ-ONLY for you): .refs/dart_pdf (the Flutter package is .refs/dart_pdf/printing),",
  "  .refs/react-native-html-to-pdf, .refs/printpdf, .refs/nsh, .refs/biometrics. docs/decisions.md records the",
  "  decided library, NSH version and platform scope; do not change it.",
  "",
  "BEHAVIOR (every call resolves exactly once: success, a coded error, or a coded cancel; bridges never panic and",
  "never hang; one print/share/convert job runs at a time per Printing instance; a late backend answer after cancel",
  "or timeout is ignored; options.timeoutMs bounds every wait and cancels the job).",
  "",
  "ERROR ENUM (Rust ErrorKind -> stable code; TS union of the same strings; messages identical wording):",
  "  NotAvailable     ERR_PDF_EXPORT_NOT_AVAILABLE      printing/dialogs do not exist here (web, unknown target)",
  "  NotSupported     ERR_PDF_EXPORT_NOT_SUPPORTED      exists in the references, deferred in v1 (raster, macOS print, Android print, HTML without an engine)",
  "  Cancelled        ERR_PDF_EXPORT_CANCELLED          the user cancelled a dialog or job",
  "  Timeout          ERR_PDF_EXPORT_TIMEOUT            the options timeout passed with the job unanswered",
  "  InProgress       ERR_PDF_EXPORT_IN_PROGRESS        another job is already running",
  "  NoActivity       ERR_PDF_EXPORT_NO_ACTIVITY        no window to attach a dialog to",
  "  Backend          ERR_PDF_EXPORT_BACKEND            the platform print service failed; carries status: Option<i64>",
  "  InvalidArgument  ERR_PDF_EXPORT_INVALID_ARGUMENT   empty pages/html/name, path separators in a file name, non-positive timeoutMs",
  "  WriteFailed      ERR_PDF_EXPORT_WRITE_FAILED       the directory is missing or the file cannot be written",
  "  FontEmbedFailed  ERR_PDF_EXPORT_FONT_EMBED_FAILED  font bytes missing or not a readable font",
  "Rust: #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)] pub struct PdfExportError { kind, status: Option<i64> }",
  "  with new(kind), with_status(i64), kind(), status(), code() -> &'static str; ErrorKind::code()/message() shaped like",
  "  .refs/biometrics/crates/nsh-biometrics/src/error.rs. TS: PdfExportError extends Error with readonly code,",
  "  a MESSAGES table with the identical wording, pdfExportError(code), and asPdfExportError(unknown) that parses a",
  "  host rejection whose message starts with the code (copy .refs/biometrics/packages/nsh-biometrics/src/errors.ts).",
  "",
  "SHARED TYPES (Rust names; TS mirrors with camelCase fields; Rust defaults shown):",
  "  Printer { name: String, url: String, is_default: bool, is_available: bool }",
  "  PrintInfo { can_print, can_list_printers, can_pick_printer, can_share, can_convert_html, can_save: bool }",
  "  PageFormat { A4 | A3 | A5 | Letter | Legal } with millimetre sizes; PageFormat::locale_default(locale: &str):",
  "    Letter for en-US/en-CA/en-MX, A4 for everything else (the brief's locale edge case). TS: type PageFormat =",
  '    "a4" | "a3" | "a5" | "letter" | "legal" and localeDefault(locale: string).',
  "  PdfPage { lines: Vec<String> } — one page of plain text lines (v1's creation input).",
  "  LayoutOptions { name = \"Document\", format = A4, use_printer_settings = false, timeout: Option<Duration> }",
  "    TS LayoutOptions { name?, format?, usePrinterSettings?, timeoutMs? } (Flutter layoutPdf/directPrintPdf's names).",
  "  ShareOptions { filename = \"document.pdf\", subject: Option<String>, body: Option<String>, emails: Vec<String> }",
  "    (Flutter sharePdf's names).",
  "  SaveOptions { directory: Option<String>, file_name = \"document.pdf\", ask_where = false } (RN fileName/directory).",
  "  ConvertOptions { base_url: Option<String>, format = A4, padding: Option<f64> mm, background = false }",
  "    (RN baseURL/format/padding/shouldPrintBackgrounds).",
  "  CreateOptions { format = A4, font: Option<FontSource> }; FontSource { name: String, bytes: Vec<u8> }.",
  "  HtmlPdf { bytes: Vec<u8>, page_count: u32 } (RN's PDFResult numberOfPages); Saved { path: String } (RN filePath).",
  "",
  "RUST Backend trait (crates/nsh-pdf-export/src/backend.rs, the platform seam, biometrics' shape):",
  "  pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;",
  "  pub type JobDone = Box<dyn FnOnce(Result<bool, PdfExportError>) + Send>;",
  "  pub type ConvertDone = Box<dyn FnOnce(Result<HtmlPdf, PdfExportError>) + Send>;",
  "  pub struct PrintRequest { bytes: Vec<u8>, name: String, printer: Option<Printer>, options: LayoutOptions }",
  "  pub struct ShareRequest { bytes: Vec<u8>, options: ShareOptions }",
  "  pub struct HtmlRequest { html: String, options: ConvertOptions }",
  "  trait Backend: Send + Sync {",
  "    fn info(&self) -> BackendFuture<Result<PrintInfo, PdfExportError>>;",
  "    fn list_printers(&self) -> BackendFuture<Result<Vec<Printer>, PdfExportError>>;",
  "    fn pick_printer(&self) -> BackendFuture<Result<Option<Printer>, PdfExportError>>;  // Ok(None) = user cancelled",
  "    fn print_pdf(&self, request: PrintRequest, done: JobDone) -> Result<(), PdfExportError>; // done called exactly once",
  "    fn cancel_print(&self);",
  "    fn share_pdf(&self, request: ShareRequest, done: JobDone) -> Result<(), PdfExportError>;",
  "    fn pick_save_path(&self) -> BackendFuture<Result<Option<PathBuf>, PdfExportError>>; // Ok(None) = cancelled",
  "    fn convert_html(&self, request: HtmlRequest, done: ConvertDone) -> Result<(), PdfExportError>;",
  "  }",
  "FakeBackend (fake.rs) mirrors .refs/biometrics/crates/nsh-biometrics/src/fake.rs: setters for every answer,",
  "  fail_next_print(code), pending_job()/finish_job(Result<bool,..>) -> bool, finish_convert(Result<HtmlPdf,..>),",
  "  set_save_path(Option<PathBuf>), counters (prints_started, cancels, shares, converts), last_request()/last_html().",
  "",
  "Printing class (printing.rs; the LocalAuth analog, Flutter's name): Clone; Printing::new() uses",
  "  platform::default_backend(), Printing::with_backend(Arc<dyn Backend>) for tests. Methods (each resolves once):",
  "  info() / list_printers() / pick_printer() — delegate; pick_printer's Ok(None) passes through (Flutter returns null).",
  "  create_pdf(&self, &[PdfPage], &CreateOptions) -> Result<Vec<u8>> — pure core via printpdf, no backend.",
  "  layout_pdf(&self, &[PdfPage], &LayoutOptions) -> Result<bool> — create bytes then backend.print_pdf with",
  "    printer: None; Ok(true) printed, Ok(false) the user cancelled (Flutter's contract).",
  "  direct_print_pdf(&self, &Printer, &[PdfPage], &LayoutOptions) -> Result<bool>.",
  "  share_pdf(&self, &[u8], &ShareOptions) -> Result<bool>.",
  "  save_pdf(&self, &[u8], &SaveOptions) -> Result<Saved> — directory given: create_dir_all + std::fs::write",
  "    (io::Error -> WriteFailed); else backend.pick_save_path(); a cancelled dialog is Err(Cancelled).",
  "  convert_html(&self, &str, &ConvertOptions) -> Result<HtmlPdf> — delegates to the backend (an engine exists only",
  "    where a platform has one; the FakeBackend answers what the test scripts).",
  "  raster(&self, &[u8]) -> Result<Vec<u8>> — always Err(NotSupported) in v1, documented (parity with Flutter's raster).",
  "  cancel(&self) -> bool — true when a job was running; the waiting call then resolves Err(Cancelled); a late",
  "    backend answer is dropped (biometrics' stop/late-answer pattern).",
  "  The busy gate: a second print/share/convert while one runs returns Err(InProgress) immediately and does not touch",
  "  the backend. Timeout: tokio::time::timeout wraps the wait, then cancel_print() + Err(Timeout).",
  "",
  "CREATION RULES (create.rs): pin printpdf = { version = \"0.12.8\", default-features = false } (the default html",
  "  feature pulls the azul engine; the ops API needs none of it). Build each page as printpdf::PdfPage::new(",
  "  Mm(w), Mm(h), ops) with Op::StartTextSection / Op::SetFont { font: PdfFontHandle::External(id), size: Pt(12.0) } /",
  "  Op::SetLineHeight / Op::ShowText { items: vec![TextItem::Text(line)] } / Op::EndTextSection — one text section",
  "  per page carrying the page's lines at a 12pt leading; then doc.save(&PdfSaveOptions::default(), &mut warnings).",
  "  ParsedFont::from_bytes(&bytes, 0, &mut Vec::new()) returns Option<ParsedFont>: None, or a missing FontSource when",
  "  any page has lines -> FontEmbedFailed. Zero pages -> InvalidArgument. A non-positive timeoutMs, an empty html, an",
  "  empty name, or a file_name containing '/' or '\\\\' -> InvalidArgument.",
  "",
  "PLATFORM MATRIX (each platform/<name>.rs implements the full Backend trait with coded errors; blocking syscalls",
  "  run inside tokio::task::spawn_blocking; a 1-2 line SAFETY comment on every unsafe block):",
  "  apple.rs  [cfg(target_vendor = \"apple\")]: iOS branch [cfg(target_os = \"ios\")] prints via objc2-uikit's",
  "            UIPrintInteractionController (printingItem = PDF data; the completion handler fires done exactly once:",
  "            completed && no error -> Ok(true), completed false -> Ok(false), error -> Backend with status);",
  "            macOS branch: info reports can_print = false and print/share/pick return NotSupported.",
  "            list_printers on iOS: NotAvailable (no public enumeration).",
  "  android.rs [cfg(target_os = \"android\")]: android.print needs a PrintDocumentAdapter, which only exists as a",
  "            Java class an NSH app cannot ship (docs/decisions.md #4): every job returns NotSupported, info reports",
  "            can_print = false. Still a complete, honest Backend impl.",
  "  windows.rs [cfg(target_os = \"windows\")]: winspool RAW passthrough via the `windows` crate (features",
  "            Win32_Graphics_Printing, Win32_Foundation): OpenPrinterW, StartDocPrinterW with Datatype RAW,",
  "            WritePrinter(bytes), EndDocPrinterW; a spooler failure maps to Backend with its error code in status.",
  "  linux.rs  [cfg(target_os = \"linux\")]: libcups via extern \"C\" declarations in our own file",
  "            (#[link(name = \"cups\")], no -sys crate): write bytes to a temp file, resolve the default destination",
  "            with cupsGetDests2/cupsGetDefault, cupsPrintFile onto it, free the dests, remove the temp file; a CUPS",
  "            error maps to Backend with the ipp status in status.",
  "  web.rs    [cfg(target_family = \"wasm\")]: creation works (printpdf is wasm-clean without default features) but",
  "            there is no filesystem, printer or share target: every dialog/job returns NotAvailable, info reports",
  "            can_save = can_print = can_share = can_convert_html = false.",
  "  unsupported.rs [cfg(not(any(apple, android, linux, windows, wasm)))]: the same answers as web.rs.",
  "platform/mod.rs declares the modules under those cfgs exactly like .refs/biometrics/.../platform/mod.rs and",
  "  exposes pub(crate) fn default_backend() -> Arc<dyn Backend>.",
  "",
  "DEPENDENCIES (crates/nsh-pdf-export/Cargo.toml; core deps unconditional: printpdf 0.12.8 default-features = false,",
  "  nsh-core = \"0.4.0\", thiserror = \"2\", tokio default-features = false features [\"sync\", \"time\"];",
  "  dev-dependencies tokio features [\"rt\", \"time\", \"macros\"]):",
  "  [target.'cfg(target_os = \"ios\")'.dependencies] objc2-uikit (verify the version and feature names —",
  "    e.g. UIPrintInteractionController, UIPrintInfo, NSData — from the extracted registry source that",
  "    `cargo metadata --format-version 1` points at; never invent a feature name)",
  "  [target.'cfg(target_os = \"windows\")'.dependencies] windows = { version = \"0.62\", features =",
  "    [\"Win32_Graphics_Printing\", \"Win32_Foundation\"] }",
  "  android, linux and web need no extra crates.",
  "",
  "NATIVE WIRING (native.rs, copy biometrics' native.rs shape): #[class] pub struct NativePrinting { printing:",
  "  Printing } with a #[class] impl of async methods mirroring the Printing API for TSX (plain values in and out;",
  "  for Vec<Printer> return what biometrics returns for its Vec<BiometricType> — Vec<String> of names — and keep",
  "  options in a #[derive(JsObject)] NativeSaveOptions-style struct where needed), errors map via",
  "  JsError::Error(kind.message().to_owned()).with_code(code). lib.rs ends with",
  "  #[native_module(\"nsh-pdf-export/native\")] fn native(registry: &mut Registry) { registry.class::<NativePrinting>(); }",
  "  (nsh requires this — a crate without it fails nsh check).",
  "",
  "TS PACKAGE (packages/nsh-pdf-export, mirror biometrics' package file-for-file):",
  "  package.json: name nsh-pdf-export, type module, main/types/exports ./src/index.ts (copy biometrics' fields).",
  "  tsconfig.json: { \"extends\": \"./.nsh/tsconfig.json\", \"include\": [\"App.tsx\", \"src/**/*.ts\", \"tests/**/*.ts\",",
  "    \"tests/**/*.tsx\"] }. nsh.json: { name: \"nsh-pdf-export tests\", id: \"dev.supernsh.pdftests\", version: \"0.1.0\",",
  "    build: 1, rust: \"../../crates/nsh-pdf-export\", capabilities: {} }. App.tsx uses ONLY nsh components (View, Text,",
  "    signal, mount from \"nsh\") — HTML tags like <div> do not exist in nsh's JSX and fail nsh check.",
  "  src/: errors.ts, options.ts (types), backend.ts (PrintBackend interface — promise-shaped: info, listPrinters,",
  "    pickPrinter -> Printer | null, printPdf(request) -> boolean, cancelPrint, sharePdf(request) -> boolean,",
  "    pickSavePath -> string | null, convertHtml(request) -> HtmlPdf, createPdf(pages, options) -> Uint8Array),",
  "    rust.ts (rustBackend() wrapping NativePrinting from \"nsh-pdf-export/native\", asPdfExportError on every",
  "    rejection — copy rust.ts's settle pattern), memory.ts (memoryBackend(config) the test drives: finishJob(",
  "    boolean | code) -> boolean, pendingJob(), prompts(), cancels(), lastRequest(), lastHtml(), setSavePath, plus a",
  "    minimal createPdf returning valid PDF bytes — starts %PDF-, ends %%EOF, one page per PdfPage — documented as",
  "    the no-crate backend), printing.ts (class Printing with the same method names in camelCase and the same",
  "    busy/cancel/late-answer/timeout logic as .refs/biometrics/.../src/auth.ts; createPdf goes through the",
  "    backend), index.ts (re-exports).",
  "",
  "EDGE CASES — each maps to ONE named test, same behavior in Rust (snake_case) and TS (camelCase), implemented by",
  "  the tests area from docs/goals.md: second_call_rejected_in_progress; cancel_resolves_coded_cancelled;",
  "  late_answer_ignored; timeout_ms_rejects_timeout; cancel_without_job_returns_false; empty_pages_rejected;",
  "  lines_without_font_rejected (FontEmbedFailed); garbage_font_rejected (FontEmbedFailed); created_bytes_are_pdf",
  "  (%PDF header, two pages, %%EOF); locale_default_page_format (de-DE -> a4, en-US -> letter); many_pages_succeed",
  "  (200 pages, bounded, no hang); missing_directory_write_failed; save_cancel_is_coded (pick path None ->",
  "  Cancelled); backend_status_preserved (a Backend error keeps the platform status); no_activity_rejected;",
  "  share_reports_not_available; raster_deferred_coded (always NotSupported); every_code_is_stable (all ten exact",
  "  code strings on both sides, messages non-empty); invalid_file_name_rejected (empty or with a path separator);",
  "  html_without_engine_coded (a backend in its not-supported state -> NotSupported).",
  "",
  "REPO RULES (AGENTS.md): comments are 1-2 lines and state a non-obvious constraint; dead code is deleted; no",
  "  speculative abstractions; every behavior lands with a test; docs claims must not outrun what CI verifies",
  "  (CI cannot run this session — say \"unverified by CI\" where a claim needs it). Never run emulators, simulators",
  "  or device builds. Never git commit, push, branch or worktree. Work directly in the current checkout.",
].join("\n");

// Per-area plans. Files are disjoint: no two areas write the same file.
interface AreaDef {
  key: string;
  label: string;
  specialist: string | null;
  fallback: string;
  owns: string[];
  /** Runs this area's own scoped check; the command is spelled out at each call site. */
  check: { name: string; run: () => ReturnType<typeof world.run> };
  implement: string;
  auditScope: string;
}

function shCheck(name: string, script: string, timeoutMs: number): AreaDef["check"] {
  return { name, run: () => world.run("sh", ["-c", script], { timeoutMs }) };
}

const areaDefs: AreaDef[] = [
  {
    key: "goals",
    label: "Goals doc",
    specialist: null,
    fallback:
      "You are a precise developer-documentation writer: direct, concrete, no marketing filler; every claim you write must be something the code and tests actually do.",
    owns: ["docs/goals.md"],
    check: { name: "documentation only — no command check", run: () => world.run("cargo", ["--version"], { timeoutMs: 30_000 }) },
    implement: [
      "Write docs/goals.md from scratch (the file does not exist yet) for nsh-pdf-export. Read docs/brief.md,",
      "docs/decisions.md, the CONTRACT below, and ground every row in the references: .refs/dart_pdf/printing/lib/src/",
      "printing.dart (the Flutter API), .refs/react-native-html-to-pdf/src/NativeHtmlToPdf.ts (the React Native API),",
      ".refs/biometrics/crates/nsh-biometrics/src/error.rs (the error pattern this org uses).",
      "Contents, in this order:",
      "1. Goals: what the crate and package do (create PDFs with printpdf; save to a path; print where the platform",
      "   allows; share; convert HTML where an engine exists), stated for a developer who will call the API.",
      "2. Acceptance criteria: the four checks (cargo fmt --all -- --check, cargo clippy --all-targets -- -D warnings,",
      "   cargo test --all-targets, plus nsh check / nsh build / nsh test in packages/nsh-pdf-export); every call",
      "   resolves exactly once (success, coded error, coded cancel); bridges never panic and never hang; same API",
      "   names and behavior in Rust and TS; layout mirrors SuperNSH/biometrics.",
      "3. The error enum: one table row per kind from the CONTRACT with its exact stable code string, when it fires,",
      "   and the optional platform status field. This table is normative for the Rust and TS implementations.",
      "4. Platform matrix: what each of Apple (iOS/macOS), Android, Windows, Linux, web can do in v1, per",
      "   docs/decisions.md.",
      "5. The edge-case list: every capability and failure mode the Flutter and React Native references handle",
      "   (enumerate them from the two reference files — layout, printer listing and picking, direct print, share,",
      "   convertHtml with its options fields, info, raster, file writing with directory/fileName, page size by",
      "   locale, fonts, large documents) — and map EACH case to its named test from the CONTRACT's edge-case list,",
      "   or to an explicit deferral with its code.",
      "Honesty rule: nothing here may claim CI verification — CI has not run this session. Write \"local tests pass;",
      "CI verification pending\" where a status needs it.",
    ].join("\n"),
    auditScope: "docs/goals.md",
  },
  {
    key: "parity",
    label: "Parity doc",
    specialist: null,
    fallback:
      "You are a precise developer-documentation writer: direct, concrete, no marketing filler; every claim you write must be something the code and tests actually do.",
    owns: ["docs/parity.md", "README.md"],
    check: { name: "documentation only — no command check", run: () => world.run("cargo", ["--version"], { timeoutMs: 30_000 }) },
    implement: [
      "Rewrite docs/parity.md and update README.md so every public API of the two references maps to what ours does.",
      "Sources to enumerate: .refs/dart_pdf/printing/lib/src/printing.dart (layoutPdf, listPrinters, pickPrinter,",
      "directPrintPdf, sharePdf, convertHtml, info, raster, OutputType, Printer, PrintingInfo fields, PdfPageFormat),",
      ".refs/react-native-html-to-pdf/src/NativeHtmlToPdf.ts (convert + every PDFOptions field + every PDFResult",
      "field). Map each to our API from the CONTRACT (Printing methods, options types, error codes) and state what",
      "ours does per platform where it differs. Deferred capabilities name their code (raster ->",
      "ERR_PDF_EXPORT_NOT_SUPPORTED, Android print -> NotSupported, HTML without an engine -> NotSupported) and cite",
      "docs/decisions.md.",
      "Update the existing status table: keep its honesty rule but reword the statuses to what is true this session —",
      "\"implemented (local tests; CI verification pending)\" for what the code now does, \"deferred (ERR_...)\") for",
      "coded deferrals, \"not started\" for anything genuinely not written. Per AGENTS.md a claim with no passing CI",
      "run is unverified, so no row may say plain \"done\".",
      "README.md: keep the title, one-line description, NSH set membership and Licence section; update the Status",
      "section (implemented locally, CI verification pending — no push this session), the Layout section",
      "(crates/nsh-pdf-export, packages/nsh-pdf-export, docs/, .refs/ local reference clones, site/), the Planned",
      "approach section (printpdf 0.12.8 creation, native printing where the platform allows) and the Checks section",
      "(the four checks; the CI config exists in .github/workflows/ci.yml; it has not run this session). Do not",
      "touch docs/goals.md, docs/decisions.md, site/ or any workflow file.",
    ].join("\n"),
    auditScope: "docs/parity.md, README.md",
  },
  {
    key: "rust-core",
    label: "Rust core",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: [
      "Cargo.toml (root — becomes a [workspace] manifest)",
      "crates/nsh-pdf-export/Cargo.toml",
      "crates/nsh-pdf-export/src/lib.rs, error.rs, backend.rs, fake.rs, options.rs, create.rs, printing.rs, native.rs",
      "crates/nsh-pdf-export/src/platform/mod.rs",
      "crates/nsh-pdf-export/src/platform/unsupported.rs",
      "crates/nsh-pdf-export/src/platform/apple.rs, android.rs, windows.rs, linux.rs, web.rs — INITIAL STUBS ONLY",
    ],
    check: shCheck(
      "cargo clippy + test (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo clippy --all-targets -- -D warnings && cargo test --all-targets",
      1_200_000,
    ),
    implement: [
      "ORDER MATTERS: first, before anything else, create ALL the files you own, including the five bridge stubs, so",
      "the areas that run beside you never race your writes. Then deepen the core.",
      "1. Root Cargo.toml becomes a virtual manifest exactly like .refs/biometrics/Cargo.toml: [workspace] with",
      "   members = [\"crates/nsh-pdf-export\"], resolver = \"3\", and the [workspace.package] block (version 0.1.0,",
      "   edition 2024, license MIT, repository https://github.com/SuperNSH/pdf-export, authors [\"hmziqrs\"]).",
      "2. Delete the old scaffold: remove the root src/ directory (the new crate replaces it).",
      "3. Create crates/nsh-pdf-export/Cargo.toml mirroring .refs/biometrics/crates/nsh-biometrics/Cargo.toml's shape",
      "   (workspace inheritance, [lib] name nsh_pdf_export, the dependency pins from the CONTRACT). Verify each pin",
      "   by compiling — printpdf 0.12.8 default-features = false is verified to compile on aarch64-apple-ios,",
      "   aarch64-linux-android, wasm32-unknown-unknown, x86_64-pc-windows-gnu, x86_64-unknown-linux-gnu and the macOS",
      "   host; for objc2-uikit and windows read the feature names from the extracted registry source",
      "   (cargo metadata --format-version 1 tells you the path) — never invent a feature name.",
      "4. Write src/ per the CONTRACT: error.rs (ten kinds, codes, messages, status), options.rs (all shared types +",
      "   locale_default), backend.rs (trait + request types + done callbacks), fake.rs (scriptable backend mirroring",
      "   biometrics' FakeBackend), create.rs (the printpdf ops path; ParsedFont::from_bytes returns Option -> None is",
      "   FontEmbedFailed), printing.rs (the Printing class with the busy gate, cancel, late-answer drop, timeout),",
      "   native.rs (#[class] NativePrinting + #[native_module(\"nsh-pdf-export/native\")]), platform/mod.rs (cfg",
      "   declarations + default_backend), platform/unsupported.rs, and five compilable stubs in platform/apple.rs,",
      "   android.rs, windows.rs, linux.rs, web.rs — each stub implements the Backend trait with the coded",
      "   NotAvailable/NotSupported answers from the CONTRACT matrix, and each carries a one-line comment saying the",
      "   platform area owns the real body.",
      "5. Unit tests for your own invariants live as #[cfg(test)] mod tests inside YOUR files (biometrics does this",
      "   in native.rs): kind->code completeness, locale_default mapping, options defaults, the busy gate and",
      "   late-answer drop against FakeBackend, create_pdf's InvalidArgument/FontEmbedFailed paths. The integration",
      "   tests in crates/nsh-pdf-export/tests/ belong to another area — do not create that directory.",
      "Load the skills your contract routes (rust-best-practices, rust-async-patterns, rust-testing) when the",
      "assignment needs them, and verify uncertain names in the extracted registry sources, never by analogy.",
    ].join("\n"),
    auditScope: "Cargo.toml, crates/nsh-pdf-export/Cargo.toml, crates/nsh-pdf-export/src/** (EXCEPT the five platform bridge files apple.rs/android.rs/windows.rs/linux.rs/web.rs, whose bodies the bridge areas own — audit their stub state only)",
  },
  {
    key: "bridge-apple",
    label: "Apple bridge",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: ["crates/nsh-pdf-export/src/platform/apple.rs (the whole file, replacing the stub)"],
    check: shCheck(
      "cargo clippy --target aarch64-apple-ios (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo clippy --lib --target aarch64-apple-ios -- -D warnings",
      1_200_000,
    ),
    implement: [
      "Implement the Apple platform backend in crates/nsh-pdf-export/src/platform/apple.rs. The Rust core area (which",
      "starts at the same time as you) creates the file first as a stub: if the file does not exist yet, wait for it",
      "(check every ~30 seconds, up to 10 minutes), then replace the stub with your full implementation — you own the",
      "final body.",
      "Implement the full Backend trait per the CONTRACT matrix. iOS branch (#[cfg(target_os = \"ios\")]):",
      "UIPrintInteractionController via objc2-uikit — build NSData from the PDF bytes, set printingItem, present, and",
      "in the completion handler call done exactly once with the right answer (completed && no error -> Ok(true);",
      "completed false -> Ok(false); error -> Backend with the underlying code in status). info() on iOS reports",
      "can_print = true, can_save = true (explicit paths), can_list_printers = false, can_pick_printer = false,",
      "can_share = true, can_convert_html = false; list_printers/pick_printer return NotAvailable there. macOS branch",
      "(#[cfg(target_os = \"macos\")]): info reports can_print = false and the print/share/pick jobs return",
      "NotSupported (documented deferral); pick_save_path returns NotAvailable in v1.",
      "objc2 usage follows .refs/biometrics/crates/nsh-biometrics/src/platform/apple.rs: msg_send style, 1-2 line",
      "SAFETY comments, no unwrap on platform results. The dependency is gated to cfg(target_os = \"ios\") in",
      "crates/nsh-pdf-export/Cargo.toml, which the Rust core area owns — if the objc2-uikit dependency or a feature",
      "name you need is missing or wrong, report it as a blocker in your result instead of editing that file.",
      "The file must keep compiling on BOTH the macOS host (macos branch) and aarch64-apple-ios (ios branch).",
    ].join("\n"),
    auditScope: "crates/nsh-pdf-export/src/platform/apple.rs",
  },
  {
    key: "bridge-android",
    label: "Android bridge",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: ["crates/nsh-pdf-export/src/platform/android.rs (the whole file, replacing the stub)"],
    check: shCheck(
      "cargo clippy --target aarch64-linux-android (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo clippy --lib --target aarch64-linux-android -- -D warnings",
      1_200_000,
    ),
    implement: [
      "Implement the Android platform backend in crates/nsh-pdf-export/src/platform/android.rs. The Rust core area",
      "(which starts at the same time as you) creates the file first as a stub: if the file does not exist yet, wait",
      "for it (check every ~30 seconds, up to 10 minutes), then replace the stub with your full implementation.",
      "Per docs/decisions.md #4, android.print needs a PrintDocumentAdapter that only exists as a Java class an NSH",
      "app cannot ship, so v1 is an honest coded deferral: every job method (print_pdf, share_pdf, convert_html)",
      "returns Err(NotSupported), pick_save_path returns Err(NotAvailable), cancel_print is a no-op, list_printers",
      "returns Ok(empty), pick_printer returns Ok(None), and info() reports can_print = false, can_share = false,",
      "can_convert_html = false, can_save = true (explicit paths still work through the core's std::fs write). The",
      "module doc comment states the PrintDocumentAdapter constraint in one sentence, so the next person does not",
      "re-derive it. If jni/ndk-context are not needed for that contract, the file needs no extra dependency — prefer",
      "that. The file must compile clean under cargo clippy --target aarch64-linux-android.",
    ].join("\n"),
    auditScope: "crates/nsh-pdf-export/src/platform/android.rs",
  },
  {
    key: "bridge-windows",
    label: "Windows bridge",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: ["crates/nsh-pdf-export/src/platform/windows.rs (the whole file, replacing the stub)"],
    check: shCheck(
      "cargo clippy --target x86_64-pc-windows-gnu (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo clippy --lib --target x86_64-pc-windows-gnu -- -D warnings",
      1_200_000,
    ),
    implement: [
      "Implement the Windows platform backend in crates/nsh-pdf-export/src/platform/windows.rs. The Rust core area",
      "(which starts at the same time as you) creates the file first as a stub: if the file does not exist yet, wait",
      "for it (check every ~30 seconds, up to 10 minutes), then replace the stub with your full implementation.",
      "Print via winspool RAW passthrough with the `windows` crate (features Win32_Graphics_Printing,",
      "Win32_Foundation — the dependency is gated to cfg(target_os = \"windows\") in Cargo.toml, owned by the core",
      "area; report a missing or wrongly-featured dependency as a blocker instead of editing that file):",
      "OpenPrinterW on the default destination when printer is None (or the chosen printer's name when set),",
      "StartDocPrinterW with a DOC_INFO_1W whose Datatype is \"RAW\", WritePrinter with the PDF bytes, EndDocPrinterW,",
      "ClosePrinter; guard every handle with a Drop guard like biometrics' ComGuard so early returns cannot leak;",
      "map a spooler failure to Err(Backend.with_status(the GetLastError value as i64)). list_printers via",
      "EnumPrintersW (PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS) mapping to our Printer fields (is_default from",
      "GetDefaultPrinter, is_available from the status flags); pick_printer returns NotAvailable (no picker UI in",
      "v1); share_pdf and convert_html return NotSupported; pick_save_path returns NotAvailable in v1; info() reports",
      "can_print = true, can_list_printers = true, can_pick_printer = false, can_share = false,",
      "can_convert_html = false, can_save = true.",
      "1-2 line SAFETY comments on every unsafe block, handles never leaked, done called exactly once, blocking",
      "spooler calls inside tokio::task::spawn_blocking. The file must compile clean under cargo clippy --target",
      "x86_64-pc-windows-gnu (check-only on this host; real linking happens in CI's Windows job).",
    ].join("\n"),
    auditScope: "crates/nsh-pdf-export/src/platform/windows.rs",
  },
  {
    key: "bridge-linux",
    label: "Linux bridge",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: ["crates/nsh-pdf-export/src/platform/linux.rs (the whole file, replacing the stub)"],
    check: shCheck(
      "cargo clippy --target x86_64-unknown-linux-gnu (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo clippy --lib --target x86_64-unknown-linux-gnu -- -D warnings",
      1_200_000,
    ),
    implement: [
      "Implement the Linux platform backend in crates/nsh-pdf-export/src/platform/linux.rs. The Rust core area (which",
      "starts at the same time as you) creates the file first as a stub: if the file does not exist yet, wait for it",
      "(check every ~30 seconds, up to 10 minutes), then replace the stub with your full implementation.",
      "Print via libcups with extern \"C\" declarations in our own file — #[link(name = \"cups\")] on the extern block,",
      "no -sys crate, so cargo check passes everywhere and only a real Linux link needs the library (CI's Linux job",
      "installs libcups2-dev). Flow: write the PDF bytes to a temp file under std::env::temp_dir(), resolve the",
      "default destination with cupsGetDests2/cupsGetDefault (verify the exact CUPS 2.x signature names from CUPS",
      "headers or sources you can find locally — check .refs/nsh and .refs/biometrics for any vendored cups usage",
      "first; if the refs have none, use the long-stable signatures cupsGetDests2 and cupsPrintFile, which CUPS 1.x",
      "and 2.x both export), cupsPrintFile onto it, free the dests, remove the temp file, and map a CUPS error to",
      "Err(Backend.with_status(the ipp status as i64)). done is called exactly once; blocking calls run inside",
      "tokio::task::spawn_blocking. list_printers via cupsGetDests2 mapping name/is_default/is_available; pick_printer",
      "returns NotAvailable (no picker UI in v1); share_pdf and convert_html return NotSupported; pick_save_path",
      "NotAvailable in v1; info() reports can_print = true, can_list_printers = true, can_pick_printer = false,",
      "can_share = false, can_convert_html = false, can_save = true. The file must compile clean under cargo clippy",
      "--target x86_64-unknown-linux-gnu.",
    ].join("\n"),
    auditScope: "crates/nsh-pdf-export/src/platform/linux.rs",
  },
  {
    key: "bridge-web",
    label: "Web bridge",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: ["crates/nsh-pdf-export/src/platform/web.rs (the whole file, replacing the stub)"],
    check: shCheck(
      "cargo clippy --target wasm32-unknown-unknown (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings",
      1_200_000,
    ),
    implement: [
      "Implement the web platform backend in crates/nsh-pdf-export/src/platform/web.rs. The Rust core area (which",
      "starts at the same time as you) creates the file first as a stub: if the file does not exist yet, wait for it",
      "(check every ~30 seconds, up to 10 minutes), then replace the stub with your full implementation, gated",
      "#[cfg(target_family = \"wasm\")]. On the web the crate can create PDFs in memory (printpdf without default",
      "features is wasm-clean — the core does that) but there is no filesystem, printer list, print dialog or share",
      "target: print_pdf/share_pdf/convert_html return Err(NotAvailable), pick_save_path returns Err(NotAvailable),",
      "cancel_print is a no-op, list_printers returns Ok(empty), pick_printer Ok(None), and info() reports",
      "can_print = can_list_printers = can_pick_printer = can_share = can_convert_html = can_save = false. The module",
      "doc comment says it in one sentence (creation works, persistence does not — a browser download flow is future",
      "work). Mirror .refs/biometrics/.../platform/unsupported.rs for the shape. No extra dependency. The file must",
      "compile clean under cargo clippy --target wasm32-unknown-unknown.",
    ].join("\n"),
    auditScope: "crates/nsh-pdf-export/src/platform/web.rs",
  },
  {
    key: "ts-package",
    label: "TSX package",
    specialist: "z-nsh-engineer",
    fallback: "You are a careful NSH TSX engineer: verify every API name in .nsh/types or the resolved crate source, never translate by analogy.",
    owns: [
      "packages/nsh-pdf-export/package.json, tsconfig.json, nsh.json, App.tsx",
      "packages/nsh-pdf-export/src/index.ts, errors.ts, options.ts, backend.ts, rust.ts, memory.ts, printing.ts",
    ],
    check: shCheck(
      "nsh check (packages/nsh-pdf-export)",
      "cd packages/nsh-pdf-export && nsh check",
      1_500_000,
    ),
    implement: [
      "Create the TSX package packages/nsh-pdf-export mirroring .refs/biometrics/packages/nsh-biometrics",
      "file-for-file, per the CONTRACT's TS section: package.json (name nsh-pdf-export, type module, exports),",
      "tsconfig.json (extends ./.nsh/tsconfig.json), nsh.json (rust: \"../../crates/nsh-pdf-export\"), App.tsx (nsh",
      "components only — View/Text/signal/mount; HTML tags do not exist in nsh's JSX), and src/: errors.ts (the ten",
      "codes, identical messages, asPdfExportError), options.ts (all shared types), backend.ts (PrintBackend",
      "interface), rust.ts (rustBackend() via NativePrinting from \"nsh-pdf-export/native\"), memory.ts",
      "(memoryBackend(config) with finishJob/pendingJob/counters/lastRequest and the minimal %PDF..%%EOF createPdf),",
      "printing.ts (class Printing, camelCase methods, the same busy/cancel/late-answer/timeout logic as biometrics'",
      "auth.ts), index.ts (re-exports). Create your files immediately — do not wait for the Rust crate; your check",
      "runs after you finish and links the crate, which the Rust core area owns.",
      "Run `nsh --version` first (0.4.0 is installed locally) and verify names in the generated .nsh/types, never by",
      "analogy. Your check `nsh check` typechecks the TSX and compiles the linked Rust crate for the host; failures",
      "that trace to crates/nsh-pdf-export/src/platform/*.rs are bridge areas' files still in flight — note them, do",
      "not fix them. Do not create packages/nsh-pdf-export/tests — that directory belongs to the tests area.",
    ].join("\n"),
    auditScope: "packages/nsh-pdf-export/package.json, tsconfig.json, nsh.json, App.tsx, src/** (EXCLUDING tests/)",
  },
  {
    key: "tests",
    label: "Tests and CI",
    specialist: "z-rust-engineer",
    fallback: "You are a careful Rust engineer: owned data, no unwrap on platform paths, no speculative abstractions, every error coded.",
    owns: [
      "crates/nsh-pdf-export/tests/printing.rs, create.rs, errors.rs",
      "crates/nsh-pdf-export/tests/fixtures/ (a usable .ttf font + a NOTICE line stating its source and license)",
      "packages/nsh-pdf-export/tests/printing.test.tsx, create.test.tsx, errors.test.tsx",
      ".github/workflows/ci.yml",
    ],
    check: shCheck(
      "cargo test --all-targets (crates/nsh-pdf-export)",
      "cd crates/nsh-pdf-export && cargo test --all-targets",
      1_200_000,
    ),
    implement: [
      "Write the integration tests for every goal, edge case and error path, in BOTH languages, plus the CI workflow.",
      "Create the files immediately; the areas they test run beside you and follow the same pinned CONTRACT.",
      "Rust tests in crates/nsh-pdf-export/tests/{printing.rs, create.rs, errors.rs} against Printing::with_backend +",
      "FakeBackend, async via #[tokio::test]: implement every edge-case test from the CONTRACT's list (the twenty",
      "named tests; docs/goals.md documents the mapping). For the font tests, copy a usable .ttf from",
      ".refs/printpdf/examples/assets/fonts/RobotoMedium.ttf into crates/nsh-pdf-export/tests/fixtures/ and add",
      "fixtures/NOTICE.txt with two lines (file source and license — Roboto, Apache License 2.0); if that path is",
      "missing, find another .ttf under .refs/printpdf and record where it came from.",
      "TS tests in packages/nsh-pdf-export/tests/{printing.test.tsx, create.test.tsx, errors.test.tsx} with",
      "`import { expect, test } from \"nsh/test\"` and `import { ... } from \"../src\"` against memoryBackend() — the",
      "camelCase mirror of each Rust test, same names, same assertions.",
      "Coordinate by contract, not by waiting: the Rust and TS APIs are pinned in the CONTRACT; write tests against",
      "the contract. If a test cannot compile because shipped code drifted from the contract, that drift is a FINDING",
      "for your auditor (name the file and the drift), not a reason to edit another area's file.",
      "Rewrite .github/workflows/ci.yml to run both suites, mirroring the org's shape in",
      ".refs/biometrics/.github/workflows/ci.yml (pinned action SHAs copied from it, same on/concurrency/permissions/",
      "env headers): a format job (cargo fmt --all -- --check), a rust matrix job on [ubuntu-latest, macos-latest,",
      "windows-latest] running the repo's existing checks verbatim (cargo clippy --all-targets -- -D warnings,",
      "cargo test --all-targets) with libcups2-dev installed on Linux (the Linux bridge links cups), a cross-check",
      "job (aarch64-apple-ios and aarch64-apple-ios-sim on macOS, aarch64-linux-android on Linux with biometrics' NDK",
      "env lines, wasm32-unknown-unknown on Linux) running cargo clippy -p nsh-pdf-export --target <t> --all-targets",
      "-- -D warnings, and a typescript job on ubuntu that installs NSH v0.4.0 with biometrics' install.sh block,",
      "installs libcups2-dev, and runs nsh check, nsh build 2>&1 | tee build.log, ! grep -i warn build.log, nsh test",
      "inside packages/nsh-pdf-export. No demo/publish/pages jobs — out of scope this task. Leave",
      ".github/workflows/pages.yml untouched.",
    ].join("\n"),
    auditScope: "crates/nsh-pdf-export/tests/**, packages/nsh-pdf-export/tests/**, .github/workflows/ci.yml",
  },
];

artifact.board("areas", {
  key: "name",
  status: "phase",
  columns: ["implementing", "auditing", "fixing", "done", "failed"],
  title: "Area progress",
  description:
    "Ten areas building the Rust crate, the TSX package, the docs and CI — each runs its own 3-round audit loop before the full checks run once.",
});

function ownersOf(output: string): string[] {
  const hits = new Set<string>();
  if (/platform[/\\]apple\.rs/.test(output)) hits.add("bridge-apple");
  if (/platform[/\\]android\.rs/.test(output)) hits.add("bridge-android");
  if (/platform[/\\]windows\.rs/.test(output)) hits.add("bridge-windows");
  if (/platform[/\\]linux\.rs/.test(output)) hits.add("bridge-linux");
  if (/platform[/\\](web|unsupported)\.rs/.test(output)) hits.add("rust-core");
  if (/crates[/\\]nsh-pdf-export[/\\]src[/\\](?!platform)/.test(output)) hits.add("rust-core");
  if (/packages[/\\]nsh-pdf-export[/\\]src/.test(output)) hits.add("ts-package");
  if (/(tests[/\\][^/\\]*\.test\.tsx|packages[/\\]nsh-pdf-export[/\\]tests)/.test(output)) hits.add("tests");
  if (/crates[/\\]nsh-pdf-export[/\\]tests/.test(output)) hits.add("tests");
  if (/Cargo\.toml/.test(output)) hits.add("rust-core");
  if (/docs[/\\]goals/.test(output)) hits.add("goals");
  if (/docs[/\\]parity|README/.test(output)) hits.add("parity");
  return [...hits];
}

// This one marker covers the whole fan-out: each area's implement -> audit
// loop (with its own scoped check) -> outcome chain lives inside its callback.
phase("Build every area in parallel, each with its own audit loop");

const implementers = new Map<string, ReturnType<typeof agent>>();
log(`building ${areaDefs.length} areas, ${AUDIT_ROUNDS} audit rounds each`);

const outcomes: AreaOutcome[] = await Promise.all(
  areaDefs.map(async (def): Promise<AreaOutcome> => {
    try {
      const { persona } = await specialist(def.specialist ?? "", def.fallback);
      const implementer = agent(`${def.label} implementer`, { system: persona, model: MODELS.pro });
      const auditor = agent(`${def.label} auditor`, {
        system:
          persona +
          "\n\nYou are auditing, not implementing: do not edit any file. Report actionable findings only.",
        model: MODELS.pro,
      });
      implementers.set(def.key, implementer);

      report({ name: def.label, phase: "implementing", summary: def.owns.join("; ") }, "areas");
      const built = await askedWithContract<StepResult>(
        implementer,
        persona,
        [
          `Implement the "${def.label}" area of nsh-pdf-export.`,
          `You OWN exactly these files (edit nothing else): ${def.owns.join("; ")}.`,
          def.implement,
          `Repo rules: work directly in the current checkout at /Users/hmziq/fo/nsh-pdf-export — no branches, no`,
          `worktrees, no commits, no pushes, no git commands that mutate state. Do not run the whole-tree suite; the`,
          `workflow runs your own scoped check (${def.check.name}) after you. Never run emulators, simulators or`,
          `device builds. The reference clones live under .refs/ — read them, never modify them. If another area's`,
          `in-flight state blocks you, say so in blockers rather than touching files you do not own. If your check`,
          `cannot pass at all, escalate and say so plainly rather than working around it.`,
          `CONTRACT:`,
          CONTRACT,
        ].join("\n"),
      );

      let delivered = built.delivered;
      let converged = false;
      let rounds = 0;
      let summary = built.value.summary;
      for (let round = 1; round <= AUDIT_ROUNDS; round++) {
        rounds = round;
        report({ name: def.label, phase: "auditing", summary: `audit round ${round}` }, "areas");
        const g = await def.check.run();
        const gateOk = g.exitCode === 0;
        const gateOutput = tail(g.stdout + "\n" + g.stderr);
        // A failure that only names another area's files is that area's business,
        // not this round's problem — mid-flight states are expected here.
        const failedOwners = gateOk ? [] : ownersOf(gateOutput);
        const gateClean = gateOk || !failedOwners.includes(def.key);
        const gateNote = gateOk
          ? `check passed: ${def.check.name}`
          : `check FAILED (exit ${g.exitCode}, failures point at: ${failedOwners.length > 0 ? failedOwners.join(", ") : "unattributed"}): ${def.check.name}\n${gateOutput}`;
        const audit = await askedWithContract<AuditResult>(
          auditor,
          persona,
          [
            `Audit round ${round} of the "${def.label}" area. Scope: ONLY ${def.auditScope}. Read every file in scope`,
            `fully, and AGENTS.md at the repo root for the repo rules.`,
            `Deterministic check, already run by the workflow: ${gateNote}`,
            `Find every real problem: contract violations (the CONTRACT follows), panics or possible hangs, missing`,
            `coded errors, API names drifting from the contract, AGENTS.md violations (comments over 2 lines,`,
            `speculative abstractions, dead code), docs claims that outrun what CI verifies, tests that assert`,
            `nothing. Style-only preferences are not findings. A problem that lives only in files another area owns`,
            `goes in "external" — a still-in-flight sibling is normal here; call it external unless it is already`,
            `final and wrong. Return findings only, each with where/what/evidence/severity. Zero findings means this`,
            `area is clean.`,
            `CONTRACT:`,
            CONTRACT,
          ].join("\n"),
        );
        delivered = delivered && audit.delivered;
        const clean = gateClean && audit.value.findings.length === 0;
        if (clean) {
          converged = true;
          summary =
            `${summary} — clean on audit round ${round}.` +
            (audit.value.external.length > 0 ? ` External notes: ${audit.value.external.join("; ")}` : "");
          break;
        }
        report({
          name: def.label,
          phase: "fixing",
          summary: `round ${round}: ${audit.value.findings.length} finding(s)` + (gateClean ? "" : " + failing check"),
        }, "areas");
        const fixed = await implementer.ask<StepResult>(
          [
            `Fix round ${round} for the "${def.label}" area. You still own exactly: ${def.owns.join("; ")}.`,
            gateNote,
            `Findings (fix every one, in your files only):`,
            ...audit.value.findings.map((f) => `- [${f.severity}] ${f.where}: ${f.what} — evidence: ${f.evidence}`),
            audit.value.external.length > 0
              ? `External notes (do NOT fix — another area owns them): ${audit.value.external.join("; ")}`
              : "",
            `When done, return the same StepResult shape with an updated summary.`,
          ].filter(Boolean).join("\n"),
        );
        summary = fixed.summary;
      }

      const status: AreaOutcome["status"] = converged ? (rounds === 1 ? "done" : "converged") : "cap";
      const outcome: AreaOutcome = {
        name: def.label,
        status,
        summary: converged
          ? summary
          : `${summary} — DID NOT CONVERGE: ${AUDIT_ROUNDS} audit rounds ran and findings or a failing check remain.`,
        rounds,
        personaDelivered: delivered ? "verified" : "unverified",
      };
      report({ name: def.label, phase: status === "cap" ? "failed" : "done", summary: outcome.summary }, "areas");
      return outcome;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const outcome: AreaOutcome = {
        name: def.label,
        status: "failed",
        summary: `Area failed: ${message}`,
        rounds: 0,
        personaDelivered: "unverified",
      };
      report({ name: def.label, phase: "failed", summary: outcome.summary }, "areas");
      return outcome;
    }
  }),
);

// The one join that genuinely needs every area: the full checks over the tree.
phase("Run the full checks once over the whole tree");

async function runGates(): Promise<GateResult[]> {
  const fmt = await world.run("cargo", ["fmt", "--all", "--", "--check"], { timeoutMs: 180_000 });
  const clippy = await world.run("cargo", ["clippy", "--all-targets", "--", "-D", "warnings"], { timeoutMs: 1_200_000 });
  const test = await world.run("cargo", ["test", "--all-targets"], { timeoutMs: 1_200_000 });
  const nshBuild = await world.run("sh", ["-c", "cd packages/nsh-pdf-export && nsh check && nsh build 2>&1 | tee build.log && ! grep -i warn build.log"], { timeoutMs: 1_500_000 });
  const nshTest = await world.run("sh", ["-c", "cd packages/nsh-pdf-export && nsh test"], { timeoutMs: 1_800_000 });
  const results: GateResult[] = [
    { name: "cargo fmt --all -- --check", ok: fmt.exitCode === 0, exitCode: fmt.exitCode, tail: tail(fmt.stdout + "\n" + fmt.stderr) },
    { name: "cargo clippy --all-targets -- -D warnings", ok: clippy.exitCode === 0, exitCode: clippy.exitCode, tail: tail(clippy.stdout + "\n" + clippy.stderr) },
    { name: "cargo test --all-targets", ok: test.exitCode === 0, exitCode: test.exitCode, tail: tail(test.stdout + "\n" + test.stderr) },
    { name: "nsh check && nsh build (packages/nsh-pdf-export)", ok: nshBuild.exitCode === 0, exitCode: nshBuild.exitCode, tail: tail(nshBuild.stdout + "\n" + nshBuild.stderr) },
    { name: "nsh test (packages/nsh-pdf-export)", ok: nshTest.exitCode === 0, exitCode: nshTest.exitCode, tail: tail(nshTest.stdout + "\n" + nshTest.stderr) },
  ];
  for (const r of results) log(`${r.ok ? "PASS" : "FAIL"}: ${r.name}`);
  return results;
}

let gates = await runGates();

// One bounded repair round: hand each failing check to the areas its output
// names, then re-run every check so the final state is one coherent snapshot.
if (gates.some((g) => !g.ok)) {
  phase("Repair the failing checks once and re-run them");
  for (const gate of gates.filter((g) => !g.ok)) {
    const owners = ownersOf(gate.tail);
    for (const key of owners.length > 0 ? owners : ["rust-core"]) {
      const ag = implementers.get(key);
      if (!ag) continue;
      await ag.ask<StepResult>(
        [
          `The whole-tree check "${gate.name}" failed after every area finished. Your area "${key}" appears in its`,
          `output; fix ONLY the failures in files your area owns (your ownership from your implement ask).`,
          `Check output:`,
          gate.tail,
          `Do not commit, push, branch, or touch files outside your ownership.`,
        ].join("\n"),
      );
    }
  }
  gates = await runGates();
}

const passCount = gates.filter((g) => g.ok).length;
const failedAreas = outcomes.filter((a) => a.status === "failed" || a.status === "cap");
const conclusion = [
  `Ten areas ran in parallel on GLM-5.3, each with its own ${AUDIT_ROUNDS}-round audit loop: ` +
    `${outcomes.filter((a) => a.status === "done").length} clean on the first audit, ` +
    `${outcomes.filter((a) => a.status === "converged").length} converged after fixes, ` +
    `${failedAreas.length} did not converge.`,
  `Whole-tree checks: ${passCount}/5 pass (fmt, clippy, cargo test, nsh check+build, nsh test).` +
    (passCount === 5 ? "" : ` Still failing: ${gates.filter((g) => !g.ok).map((g) => g.name).join(", ")}.`),
  `Nothing was committed or pushed, so CI has not verified any of this — the checks ran locally in-session only.`,
].join(" ");

await artifact.markdown(
  "report",
  [
    `# nsh-pdf-export build-out — per-area report`,
    "",
    conclusion,
    "",
    `## Areas`,
    ...outcomes.map(
      (a) =>
        `- **${a.name}** — ${a.status} (audit rounds: ${a.rounds}, persona ${a.personaDelivered}) — ${a.summary}`,
    ),
    "",
    `## Whole-tree checks`,
    ...gates.map(
      (g) =>
        `- ${g.ok ? "PASS" : "FAIL"} — ${g.name}` +
        (g.ok ? "" : ` (exit ${g.exitCode})\n  ${g.tail.split("\n").slice(0, 6).join("\n  ")}`),
    ),
    "",
    `## Scope notes`,
    `- No commits, no pushes: every result is local; the CI config (.github/workflows/ci.yml, rewritten by the tests area) has not run.`,
    `- Windows, Linux and Android bridge code was checked with cross-target clippy only; real linking and device runs are CI-only per AGENTS.md.`,
    `- The five reference repos are cloned under .refs/ (git-ignored); docs/decisions.md records the URL verification and the library/platform decisions.`,
  ].join("\n"),
  { title: "Build-out report", description: "Per-area outcomes and whole-tree check results for the nsh-pdf-export workflow.", primary: true },
);

return {
  conclusion,
  areas: outcomes,
  checks: gates.map((g) => ({ name: g.name, ok: g.ok })),
  verified: gates.filter((g) => g.ok).map((g) => `whole-tree check passed: ${g.name}`),
  notCovered: [
    ...gates.filter((g) => !g.ok).map((g) => `whole-tree check FAILED, unresolved: ${g.name}`),
    "CI has not run (no pushes allowed this session): every status claim is local-only by AGENTS.md's rule",
    "device surfaces (iOS, Android, Windows, Linux printing) were compile-checked cross-target, never executed — real runs are CI-only per AGENTS.md",
  ],
  notes: [
    "Model pins: every implementer and auditor ran on account:zai-individual-coding-plan/GLM-5.3$high per the brief.",
    `Audit budget: ${AUDIT_ROUNDS} rounds per area, stopping early on a clean round; areas still red at the cap are reported honestly.`,
    "The five reference repos were shallow-cloned into .refs/ at step 0; all five URLs resolved canonically (recorded in docs/decisions.md).",
  ],
};
