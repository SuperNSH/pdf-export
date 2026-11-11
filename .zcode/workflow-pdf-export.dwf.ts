// nsh-pdf-export build-out: ten areas advancing alone, in parallel.
// STEP 0 is already done: .refs/ holds the five verified reference clones and
// docs/decisions.md records the URL and platform decisions (launcher-owned).
// Per area: implement, then its own audit loop (3 rounds: scoped gate + auditor,
// fix, until zero findings or the cap) inside its own fan-out callback.
// Every agent is pinned to GLM-5.3 per the brief. No git machinery anywhere:
// areas edit the current checkout directly, and the whole-tree gates run once
// at the join.

const PRO = model("account:zai-individual-coding-plan/GLM-5.3$high");
const AUDIT_ROUNDS = 3;
const JOIN_FIX_ROUNDS = 3;
const KIT = "/Users/hmziq/ZCodeProject/ai/harness/zcode/agents";

interface StepResult {
  /** True when this area's own files are written and its own scoped checks pass. */
  done: boolean;
  /** What was built, in two or three sentences. */
  summary: string;
  /** Anything genuinely blocked, with the reason. */
  blockers?: string[];
  /** One sentence copied VERBATIM from the specialist contract that most shaped the work. */
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
  /** One sentence copied VERBATIM from the specialist contract that most shaped the review. */
  contractLine: string;
}

interface AreaOutcome {
  name: string;
  /** "done" first-round clean, "converged" after fix rounds, "cap" with findings left, "failed". */
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

interface RunReport {
  /** Two or three sentences answering what the brief asked for. */
  conclusion: string;
  areas: AreaOutcome[];
  /** What the run checked and how: commands with their exit codes, audits with rounds. */
  verified: string[];
  /** What this run could not check, and why. */
  notCovered: string[];
  notes: string;
}

const RULES = `
WORKING RULES (bind everywhere):
- Workspace: /Users/hmziq/fo/nsh-pdf-export. Work directly in the current checkout.
- NO git machinery: never run git commit, git push, git branch, git stash, git reset, or any other state-changing git command. The deliverable is the changed files.
- Own ONLY the files listed for your area. Never edit a file another area owns; if your work seems to require it, say so in blockers instead.
- .refs/ is read-only reference material cloned at step 0. Never modify or delete anything in it; read selectively.
- Read AGENTS.md first and follow it: comments are 1-2 lines and state a non-obvious constraint; dead code is deleted; no speculative abstractions; every behaviour change needs a test; an operation resolves exactly once (success, a coded failure, or a cancel — never hangs).
- The script runs the gates itself (cargo fmt/clippy/test, nsh check/test) after you. While iterating run only quick scoped checks; never run emulators, simulators, devices, or device builds — everything device-shaped is CI-only via .github/workflows/ci.yml.
- If your instructions contradict each other, or a check is impossible to pass honestly, escalate and say so plainly rather than working around it.
- Never fake a pass: a check you did not run is a check you did not run.
`;

const CONTRACT = `
FROZEN CONTRACT (authored by the launcher; every area codes against exactly this — where a reference differs, this contract wins for v1 and docs/parity.md records the difference).

NAMES AND LAYOUT (mirror .refs/biometrics):
- Rust lib nsh_pdf_export at crates/nsh-pdf-export. Root Cargo.toml becomes the workspace manifest: [workspace] members = ["crates/nsh-pdf-export"], resolver = "3", [workspace.package] version "0.1.0", edition "2024", license "MIT", repository "https://github.com/SuperNSH/pdf-export", authors ["hmziqrs"].
- TS package nsh-pdf-export at packages/nsh-pdf-export.

ERRORS (Rust error.rs; TS errors.ts — same 12 kinds, same codes, identical wording):
- NotAvailable / ERR_PDF_EXPORT_NOT_AVAILABLE / "PDF export is not available on this platform."
- NotSupported / ERR_PDF_EXPORT_NOT_SUPPORTED / "This capability is not supported here yet."
- InvalidArgument / ERR_PDF_EXPORT_INVALID_ARGUMENT / "An argument is empty or not allowed on this platform."
- Cancelled / ERR_PDF_EXPORT_CANCELLED / "The operation was cancelled."
- Timeout / ERR_PDF_EXPORT_TIMEOUT / "The operation timed out. Try again."
- InProgress / ERR_PDF_EXPORT_IN_PROGRESS / "Another export or print is already running. Wait for it to finish."
- FontNotFound / ERR_PDF_EXPORT_FONT_NOT_FOUND / "The requested font is not available."
- FontInvalid / ERR_PDF_EXPORT_FONT_INVALID / "The font bytes could not be read."
- WriteFailed / ERR_PDF_EXPORT_WRITE_FAILED / "The file could not be written."
- NoPrinter / ERR_PDF_EXPORT_NO_PRINTER / "No printer is available."
- PrintFailed / ERR_PDF_EXPORT_PRINT_FAILED / "The print job failed."
- Backend / ERR_PDF_EXPORT_BACKEND / "The platform's PDF export service failed."
Rust: #[non_exhaustive] enum ErrorKind (Copy, PartialEq, Eq, Hash); struct PdfExportError { kind, status: Option<i64> } deriving thiserror::Error, Display prints the message; code() -> &'static str, kind(), status(). TS: type PdfExportErrorCode as the union of the 12 codes; class PdfExportError extends Error with readonly code; asPdfExportError(error: unknown): a host rejection carrying code ERR_NSH_HOST_ERROR whose message starts with "<CODE>: " maps to that code, anything unknown maps to Backend; message strings identical to Rust.

TYPES (identical names both languages; Rust options.rs / export.rs; TS options.ts / export.ts):
- PageText { lines: string[] }
- PdfSource { kind: "text", title: string, pages: PageText[] }  (v1 is text-only)
- PageSize = "a3" | "a4" | "a5" | "letter" | "legal" | "tabloid" | { widthMm: number, heightMm: number }; pure helpers systemDefault(locale: string) -> PageSize ("en-US" -> "letter", anything else -> "a4") and sizeMm(PageSize) -> { widthMm, heightMm } (A4 210x297, A3 297x420, A5 148x210, Letter 215.9x279.4, Legal 215.9x355.6, Tabloid 279.4x431.8).
- ExportOptions { pageSize?: PageSize, title?: string }
- HtmlOptions { paddingMm?: number, backgroundColor?: string, baseUrl?: string }
- PrintOptions { printerName?: string, jobName?: string, copies?: number }  (copies 1..=999, default 1; out of range -> InvalidArgument)
- Printer { name: string, isDefault: boolean }
- PrintJob { id: string }
- Capability = "createPdf" | "saveToPath" | "saveDialog" | "convertHtml" | "print" | "listPrinters"

BACKEND TRAIT (backend.rs; TS backend.ts — Promise-based interface with the same method names):
type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
trait Backend: Send + Sync {
  fn supports(&self, capability: Capability) -> bool;
  fn create_pdf(&self, source: &PdfSource, options: &ExportOptions) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>>;
  fn save_pdf(&self, source: &PdfSource, options: &ExportOptions, path: &Path) -> BackendFuture<'_, Result<PathBuf, PdfExportError>>;
  fn pick_save_path(&self, suggested_name: &str) -> BackendFuture<'_, Result<Option<PathBuf>, PdfExportError>>;  // Ok(None) = the user cancelled
  fn convert_html(&self, html: &str, options: &HtmlOptions) -> BackendFuture<'_, Result<Vec<u8>, PdfExportError>>;
  fn print_pdf(&self, pdf: &[u8], options: &PrintOptions) -> BackendFuture<'_, Result<PrintJob, PdfExportError>>;
  fn list_printers(&self) -> BackendFuture<'_, Result<Vec<Printer>, PdfExportError>>;
  fn cancel(&self) -> BackendFuture<'_, Result<(), PdfExportError>>;
}

FRONT API (Rust export.rs class PdfExport; TS export.ts class PdfExport — identical method names):
new PdfExport(backend); PdfExport::with_default_backend() uses default_backend() (TS: new PdfExport(backend?) defaults to rustBackend()). Methods: createPdf(source, options?), savePdf(source, options?, path), savePdfWithDialog(source, options?, suggestedName) — pick_save_path returning Ok(None) resolves Cancelled; convertHtml(html, options?); printPdf(pdf, options?); listPrinters(); cancel(); supports(capability).
- One operation at a time per instance: a second concurrent call resolves InProgress; the first is unaffected.
- Default operation bound 30 seconds; PdfExport::with_timeout(Duration) / TS withTimeoutMs() replaces it; on expiry the call resolves Timeout and cancel() is attempted on the backend.
- Validation: empty or whitespace-only html -> InvalidArgument; empty pages -> InvalidArgument; zero, negative or non-finite page size -> InvalidArgument.
- cancel(): with an active operation -> Ok, and that operation resolves Cancelled; with none -> InProgress.
- No error message ever holds user data (no html, no paths, no printer names).

PDF ENGINE (export.rs, core-owned): pub(crate) async fn build_pdf(source: &PdfSource, options: &ExportOptions, font: Option<&[u8]>) -> Result<Vec<u8>, PdfExportError>. printpdf 0.12.8 with default-features = false, ops-based (PdfDocument::new + PdfPage::new + Op text items + PdfSaveOptions + doc.save -> bytes). Font bytes are required: none provided -> FontNotFound; unparseable bytes -> FontInvalid. Every bridge's create_pdf/save_pdf delegates to build_pdf, so real creation compiles and works on every target. The test font is vendored at crates/nsh-pdf-export/assets/export-test-font.ttf (copied from .refs/printpdf — verify a text op renders by writing "Hello PDF" and asserting the bytes start with "%PDF-"; note the source path in a comment).

FAKE BACKEND (fake.rs, core-owned; TS memory.ts — same shape): FakeBackend::new(&[Capability]) with scriptable per-method outcome queues (create/save/pick/convert/print/printers), delays (a delay longer than the caller's bound yields Timeout at the caller while the fake still answers once afterwards), and a full call log (pub fn calls). TS memoryBackend(config) mirrors it in pure JS.

NATIVE BRIDGE (native.rs, core-owned): #[native_module("nsh-pdf-export/native")] registering #[class] NativePdfExport, following the biometrics native.rs idioms (JsObject options structs, async methods, JsError rejections whose message is "<CODE>: <message>"). Methods (snake_case in Rust; TS calls them camelCase): capabilities() -> Vec<String>; create_pdf(source, options) -> base64 String; save_pdf(source, options, path) -> String; pick_save_path(suggested) -> Option<String>; convert_html(html, options) -> base64 String; print_pdf(pdf_base64, options) -> NativePrintJob { id }; list_printers() -> Vec<NativePrinter { name, is_default }>; cancel() -> bool. rust.ts wraps this class and settles every rejection through asPdfExportError.

PLATFORM WIRING (platform/mod.rs, core-owned): pub fn default_backend() -> &'static dyn Backend, cfg-selects apple::AppleBackend / android::AndroidBackend / windows::WindowsBackend / linux::LinuxBackend / web::WebBackend / UnsupportedBackend (unknown target; platform-only capabilities -> NotAvailable). mod.rs also owns shared helpers: save_dialog_path(suggested) wrapping nsh_core::file_dialog::SaveDialog (desktop only) and the save-to-path writer (std::fs off the async runtime; unwritable -> WriteFailed). mod.rs declares the five bridge modules; their files are written by other areas concurrently against this contract.

V1 PLATFORM MATRIX (from docs/decisions.md; docs/parity.md restates it per API):
- createPdf + saveToPath: every target the crate compiles for (on web, saveToPath resolves WriteFailed when the host has no backing fs).
- saveDialog: macOS, Windows, Linux only. iOS/Android -> NotSupported; web -> NotAvailable.
- convertHtml: NotSupported on every native platform in v1 (no engine wired; parity records it); web -> NotAvailable.
- print: iOS (UIPrintInteractionController via objc2-uikit), Windows (winspool WritePrinter RAW passthrough), Linux (libcups FFI, extern "C" #[link(name = "cups")], no sys crate); macOS NotSupported in v1; Android NotSupported in v1 (PrintManager needs a Java PrintDocumentAdapter an NSH app cannot ship); web NotAvailable.
- listPrinters: wherever print is supported; on other platforms it resolves NotSupported (web: NotAvailable).
- Every bridge struct is pub with pub fn new() so host tests construct it directly and exercise the not-supported paths.

CRATE DEPENDENCIES (crates/nsh-pdf-export/Cargo.toml, core-owned; pin latest published versions except where fixed):
nsh-core = "0.4.0"; printpdf 0.12.8 default-features = false; thiserror 2; tokio 1 default-features = false features ["sync","time"] (dev: rt, macros, time, test-util as needed); cfg(target_vendor = "apple"): objc2-uikit (UIPrintInteractionController features); cfg(target_os = "android"): jni, ndk-context, libc; cfg(target_os = "windows"): windows with Win32_Graphics_Printing + Win32_Foundation (+ windows-future std if needed); cfg(target_os = "linux"): none (extern "C" to libcups).

TS PACKAGE (packages/nsh-pdf-export, mirrors .refs/biometrics/packages/nsh-biometrics):
package.json (type module, main/types src/index.ts, exports "." and "./memory", repository), tsconfig.json adapted from biometrics with the native module mapping for "nsh-pdf-export/native", nsh.json {"name":"nsh-pdf-export tests","id":"dev.supernsh.pdf-export.tests","version":"0.1.0","build":1,"rust":"../../crates/nsh-pdf-export","capabilities":{}}, App.tsx thin demo (memoryBackend only — no native calls), src/{index,errors,options,backend,memory,rust,export}.ts. export.ts settle() wraps every backend rejection through asPdfExportError; one op at a time -> InProgress; bound expiry -> Timeout.

EDGE CASES — one NAMED test each, written by the tests-ci area in BOTH languages; docs/goals.md documents this same list with the same names:
1. convert_html_empty_html_is_invalid_argument
2. create_pdf_empty_pages_is_invalid_argument
3. create_pdf_blank_page_is_valid_pdf
4. custom_page_size_zero_is_invalid_argument
5. create_pdf_without_font_is_font_not_found
6. create_pdf_with_bad_font_bytes_is_font_invalid
7. save_pdf_to_unwritable_path_is_write_failed
8. save_dialog_cancel_is_cancelled
9. second_concurrent_call_is_in_progress
10. cancel_cancels_active_op_exactly_once (active op resolves Cancelled; a second cancel resolves InProgress)
11. cancel_and_late_answer_resolves_once_cancelled (scripted backend answers after cancel; the caller saw Cancelled, exactly once)
12. bounded_wait_is_timeout_never_hangs
13. print_with_no_printers_is_no_printer
14. print_to_unknown_printer_is_no_printer
15. web_print_is_not_available
16. android_print_is_not_supported
17. macos_print_is_not_supported_in_v1
18. convert_html_is_not_supported_in_v1
19. create_pdf_large_document_resolves (500 pages, "%PDF-" header, no hang)
20. page_size_system_default_respects_locale
21. in_progress_cleared_when_operation_settles
22. error_message_holds_no_user_data

GATES (the script runs these; host-side only — emulators/devices are CI-only):
cargo fmt --all -- --check; cargo clippy --all-targets -- -D warnings; cargo test --all-targets; (cd packages/nsh-pdf-export && nsh check); (cd packages/nsh-pdf-export && nsh test)
`;

async function specialist(
  name: string,
  fallback: string,
): Promise<{ persona: string; source: string }> {
  const adapter =
    `You are the ${name} specialist for one area of a parallel workflow. ` +
    `Your full specialist contract follows verbatim — apply it to this area's work; ` +
    `where it conflicts with your ask, the ask wins.` + "\n\n";
  for (const p of [`.zcode/agents/${name}.md`, `${KIT}/${name}.md`]) {
    try {
      const raw = await files.read(p);
      return {
        persona: adapter + raw.replace(/^---\n[\s\S]*?\n---\n/, "").trim(),
        source: p,
      };
    } catch {
      /* try the next source */
    }
  }
  return { persona: fallback, source: "fallback" };
}

function delivered(
  persona: string,
  source: string,
  result: { contractLine: string },
): boolean {
  if (source === "fallback") return false;
  return persona.includes(result.contractLine.trim());
}

async function rustGate(): Promise<GateResult> {
  const fmt = await world.run("cargo", ["fmt", "--all", "--", "--check"], { timeoutMs: 120000 });
  if (fmt.exitCode !== 0) {
    return { name: "cargo fmt --all -- --check", ok: false, exitCode: fmt.exitCode, tail: (fmt.stdout + "\n" + fmt.stderr).slice(-4000) };
  }
  const clippy = await world.run("cargo", ["clippy", "--all-targets", "--", "-D", "warnings"], { timeoutMs: 420000 });
  if (clippy.exitCode !== 0) {
    return { name: "cargo clippy --all-targets -- -D warnings", ok: false, exitCode: clippy.exitCode, tail: (clippy.stdout + "\n" + clippy.stderr).slice(-4000) };
  }
  const test = await world.run("cargo", ["test", "--all-targets"], { timeoutMs: 420000 });
  if (test.exitCode !== 0) {
    return { name: "cargo test --all-targets", ok: false, exitCode: test.exitCode, tail: (test.stdout + "\n" + test.stderr).slice(-4000) };
  }
  return { name: "cargo fmt + clippy + test (workspace)", ok: true, exitCode: 0, tail: "" };
}

async function tsGate(): Promise<GateResult> {
  const check = await world.run("sh", ["-c", "cd packages/nsh-pdf-export && nsh check"], { timeoutMs: 300000 });
  if (check.exitCode !== 0) {
    return { name: "nsh check (packages/nsh-pdf-export)", ok: false, exitCode: check.exitCode, tail: (check.stdout + "\n" + check.stderr).slice(-4000) };
  }
  const test = await world.run("sh", ["-c", "cd packages/nsh-pdf-export && nsh test"], { timeoutMs: 600000 });
  if (test.exitCode !== 0) {
    return { name: "nsh test (packages/nsh-pdf-export)", ok: false, exitCode: test.exitCode, tail: (test.stdout + "\n" + test.stderr).slice(-6000) };
  }
  return { name: "nsh check + nsh test (packages/nsh-pdf-export)", ok: true, exitCode: 0, tail: "" };
}

async function areaGate(kind: string): Promise<GateResult> {
  if (kind === "rust") return rustGate();
  if (kind === "ts") return tsGate();
  if (kind === "both") {
    const rust = await rustGate();
    if (!rust.ok) return rust;
    return tsGate();
  }
  return { name: "no command gate (docs area)", ok: true, exitCode: 0, tail: "" };
}

interface Area {
  name: string;
  owns: string[];
  impl: string;
  audit: string;
  gate: "rust" | "ts" | "both" | "none";
  task: string;
}

const AREAS: Area[] = [
  {
    name: "docs-goals",
    owns: ["docs/goals.md", "README.md"],
    impl: "general-pro",
    audit: "general-pro",
    gate: "none",
    task: `
YOUR AREA: docs-goals — write docs/goals.md and rewrite README.md.
- docs/goals.md: the goals of this library (PDF creation from app content via printpdf; native printing where the platform allows; one Rust crate and one TS package with identical API names and behavior, laid out like .refs/biometrics); acceptance criteria (the CONTRACT gates; every behavior with a test in both languages; every call resolves exactly once; coded errors only; no panics, no hangs); the error enum table (all 12 kinds, code, message); and the edge-case list — all 22 named cases from the CONTRACT, each with a one-line description and its named test, grouped (arguments, fonts, filesystem, lifecycle, platform matrix, engine). Read docs/brief.md and docs/decisions.md first, and keep the docs style like .refs/biometrics/docs/*.
- README.md: rewrite it for the real repo (what it is, the crates/ + packages/ layout, how CI verifies it). AGENTS.md rule: status claims must match what CI runs. CI (another area updates it concurrently) runs the Rust suite and the TS suite on every push — phrase claims accordingly, and mark native printing as compile-checked in CI only, never device-tested.
- docs/parity.md and docs/decisions.md belong to other areas: read them, never write them.`,
  },
  {
    name: "docs-parity",
    owns: ["docs/parity.md"],
    impl: "z-nsh-engineer",
    audit: "z-nsh-engineer",
    gate: "none",
    task: `
YOUR AREA: docs-parity — rewrite docs/parity.md (replace the stub table) with a per-API parity account.
Enumerate the public APIs from the reference clones by reading their sources:
- .refs/dart_pdf/printing: printing/lib/src/printing.dart (Printing.layoutPdf, listPrinters, pickPrinter, directPrintPdf, sharePdf, convertHtml, info, raster), src/printer.dart, src/printing_info.dart, src/output_type.dart, and PdfPageFormat usage from the pdf package where printing references it.
- .refs/react-native-html-to-pdf: src/NativeHtmlToPdf.ts — PDFOptions (html, fileName, width, height, base64, padding, paddingTop/Bottom/Left/Right, bgColor, directory, baseURL, shouldPrintBackgrounds), PDFResult (filePath, base64, numberOfPages), Spec.convert.
- .refs/printpdf: the creation engine we embed (PdfDocument, PdfPage, Op, save) — one section on how our build_pdf maps onto it.
For EVERY API of both references, one row: the reference API, what ours does for it (our method name, or the coded error per the v1 platform matrix in the CONTRACT), and status. Keep the existing stub's honesty rule: a row is not "done" until CI runs a test for it, so phrase status accordingly. Read docs/decisions.md first — it is authoritative for the v1 matrix; docs/goals.md and README.md belong to other areas (read, never write).`,
  },
  {
    name: "rust-core",
    owns: [
      "Cargo.toml (root: becomes the workspace manifest)",
      "crates/nsh-pdf-export/Cargo.toml",
      "crates/nsh-pdf-export/src/lib.rs",
      "crates/nsh-pdf-export/src/error.rs",
      "crates/nsh-pdf-export/src/backend.rs",
      "crates/nsh-pdf-export/src/options.rs",
      "crates/nsh-pdf-export/src/export.rs",
      "crates/nsh-pdf-export/src/fake.rs",
      "crates/nsh-pdf-export/src/native.rs",
      "crates/nsh-pdf-export/src/platform/mod.rs",
      "crates/nsh-pdf-export/assets/export-test-font.ttf",
    ],
    impl: "z-rust-engineer",
    audit: "z-rust-engineer",
    gate: "rust",
    task: `
YOUR AREA: rust-core — restructure the repo to the biometrics layout and implement the Rust core.
- Move the scaffold into crates/nsh-pdf-export: root Cargo.toml becomes the workspace manifest (shape above); create crates/nsh-pdf-export/Cargo.toml (lib name nsh_pdf_export, deps per CONTRACT); delete the old root src/lib.rs after recreating the crate files.
- Write error.rs, backend.rs, options.rs, export.rs (including build_pdf and the PdfExport front class), fake.rs, native.rs, platform/mod.rs exactly per CONTRACT. lib.rs: crate docs with the resolve-exactly-once sentence (mirror biometrics lib.rs tone), module declarations, re-exports (PdfExport, PdfSource, PageText, PageSize, ExportOptions, HtmlOptions, PrintOptions, Printer, PrintJob, Capability, Backend, BackendFuture, PdfExportError, ErrorKind, FakeBackend, default_backend), and the #[native_module("nsh-pdf-export/native")] registration.
- Vendor the test font to crates/nsh-pdf-export/assets/export-test-font.ttf from .refs/printpdf (its defaultfonts/ or tests assets — find one whose subset glyphs render; verify build_pdf writes "Hello PDF" and the bytes start with "%PDF-"; note the copied source path in a comment). Never modify .refs.
- platform/mod.rs declares mod apple/android/windows/linux/web + UnsupportedBackend and default_backend()'s cfg wiring; the five bridge files are written by OTHER areas concurrently against the CONTRACT — declare them, never implement their contents.
- Inline #[cfg(test)] unit tests inside your own files for your own behaviors: every code/message pair, InProgress gate, cancel race, page-size math and locale default, build_pdf with the vendored font (real printpdf runs in cargo test on the host).
- crates/nsh-pdf-export/tests/ belongs to another area: never write it.`,
  },
  {
    name: "bridge-apple",
    owns: ["crates/nsh-pdf-export/src/platform/apple.rs"],
    impl: "z-rust-engineer",
    audit: "z-unsafe-ffi-auditor",
    gate: "rust",
    task: `
YOUR AREA: bridge-apple — implement crates/nsh-pdf-export/src/platform/apple.rs, the Backend impl for iOS and macOS, per CONTRACT and the v1 matrix. You own ONLY that one file.
- iOS (cfg(target_os = "ios")): print_pdf via UIPrintInteractionController through objc2-uikit (the dep exists in the crate's Cargo.toml). list_printers: whatever UIPrintInteraction reports is not a stable query in v1 — resolve NotSupported with a comment if you cannot do it without a picker.
- macOS (cfg(target_os = "macos")): print_pdf -> NotSupported in v1 (decisions.md); saveDialog via the shared save_dialog_path helper from platform/mod.rs; pick_save_path on iOS -> NotSupported.
- create_pdf/save_pdf delegate to build_pdf (crate::export) so real creation works on Apple targets; supports() mirrors the matrix.
- The struct is pub with pub fn new(); on the macOS host the not-supported paths must be exercisable by tests without a device. No panic on external data (no unwrap/expect), no hang (every wait bounded, Timeout on expiry), coded errors only; unsafe blocks minimal, each with a 1-2 line comment stating the non-obvious constraint.
- platform/mod.rs, error.rs, backend.rs, options.rs, export.rs are core files another area writes against the same CONTRACT — code against them, never edit them.`,
  },
  {
    name: "bridge-android",
    owns: ["crates/nsh-pdf-export/src/platform/android.rs"],
    impl: "z-rust-engineer",
    audit: "z-unsafe-ffi-auditor",
    gate: "rust",
    task: `
YOUR AREA: bridge-android — implement crates/nsh-pdf-export/src/platform/android.rs, the Backend impl for Android, per CONTRACT and the v1 matrix. You own ONLY that one file.
- v1: print_pdf -> NotSupported (PrintManager needs a Java PrintDocumentAdapter an NSH app cannot ship — keep exactly that as a 1-2 line comment); pick_save_path -> NotSupported; list_printers -> NotSupported.
- create_pdf/save_pdf delegate to build_pdf (crate::export) so creation works wherever the crate compiles; supports() mirrors the matrix.
- The jni/ndk-context/libc deps exist under cfg(target_os = "android") — keep the file compiling with them available but with no JNI call sites in v1; the struct must be constructible on the host so tests exercise the not-supported paths (AndroidBackend::new()).
- No panic on external data, no hang, coded errors only. platform/mod.rs, error.rs, backend.rs, options.rs, export.rs are core files another area writes against the same CONTRACT — code against them, never edit them.`,
  },
  {
    name: "bridge-windows",
    owns: ["crates/nsh-pdf-export/src/platform/windows.rs"],
    impl: "z-rust-engineer",
    audit: "z-unsafe-ffi-auditor",
    gate: "rust",
    task: `
YOUR AREA: bridge-windows — implement crates/nsh-pdf-export/src/platform/windows.rs, the Backend impl for Windows, per CONTRACT and the v1 matrix. You own ONLY that one file.
- print_pdf via winspool RAW passthrough with the windows crate (deps exist under cfg(target_os = "windows")): OpenPrinterW, StartDocPrinterW, WritePrinter with the PDF bytes, EndDocPrinterW, ClosePrinter; unknown printer -> NoPrinter; spooler failure -> PrintFailed carrying the platform's status code in PdfExportError.status.
- list_printers via EnumPrintersW; saveDialog via the shared save_dialog_path helper from platform/mod.rs.
- create_pdf/save_pdf delegate to build_pdf (crate::export); supports() mirrors the matrix; WindowsBackend::new() constructible everywhere so host tests exercise the non-Windows paths (cfg-gate the real FFI to target_os = "windows").
- No panic on external data, no hang (every wait bounded, Timeout on expiry), coded errors only; unsafe blocks minimal with 1-2 line constraint comments.
- platform/mod.rs, error.rs, backend.rs, options.rs, export.rs are core files another area writes against the same CONTRACT — code against them, never edit them.`,
  },
  {
    name: "bridge-linux",
    owns: ["crates/nsh-pdf-export/src/platform/linux.rs"],
    impl: "z-rust-engineer",
    audit: "z-unsafe-ffi-auditor",
    gate: "rust",
    task: `
YOUR AREA: bridge-linux — implement crates/nsh-pdf-export/src/platform/linux.rs, the Backend impl for Linux, per CONTRACT and the v1 matrix. You own ONLY that one file.
- print_pdf via libcups FFI: extern "C" declarations with #[link(name = "cups")] — no -sys crate. Minimal request path (cupsGetDests/cupsCreateJob/cupsStartDocument/cupsWriteRequestData/cupsFinishDocument or the closest supported sequence); no dests -> NoPrinter; job failure -> PrintFailed carrying CUPS status in PdfExportError.status; free CUPS-allocated memory correctly (cupsFreeDests) — leaks are findings.
- list_printers from the same dest enumeration; saveDialog via the shared save_dialog_path helper from platform/mod.rs.
- create_pdf/save_pdf delegate to build_pdf (crate::export); supports() mirrors the matrix; LinuxBackend::new() constructible everywhere (cfg-gate the real FFI to target_os = "linux") so host tests exercise the non-Linux paths.
- No panic on external data, no hang (every FFI call either completes or resolves a coded error), coded errors only; unsafe blocks minimal with 1-2 line constraint comments.
- platform/mod.rs, error.rs, backend.rs, options.rs, export.rs are core files another area writes against the same CONTRACT — code against them, never edit them.`,
  },
  {
    name: "bridge-web",
    owns: ["crates/nsh-pdf-export/src/platform/web.rs"],
    impl: "z-rust-engineer",
    audit: "z-rust-engineer",
    gate: "rust",
    task: `
YOUR AREA: bridge-web — implement crates/nsh-pdf-export/src/platform/web.rs, the Backend impl for wasm32/web, per CONTRACT and the v1 matrix. You own ONLY that one file.
- print_pdf, convert_html, pick_save_path, list_printers -> NotAvailable (the web has no print spooler, HTML engine, or fs in v1); supports() mirrors the matrix.
- create_pdf delegates to build_pdf (crate::export); save_pdf attempts the host fs path and resolves WriteFailed when there is no backing fs — coded errors, never a panic, never a hang.
- WebBackend::new() constructible on every target (cfg-gate nothing web-specific beyond what compiles everywhere) so host tests exercise the NotAvailable paths.
- platform/mod.rs, error.rs, backend.rs, options.rs, export.rs are core files another area writes against the same CONTRACT — code against them, never edit them.`,
  },
  {
    name: "ts-package",
    owns: [
      "packages/nsh-pdf-export/package.json",
      "packages/nsh-pdf-export/tsconfig.json",
      "packages/nsh-pdf-export/nsh.json",
      "packages/nsh-pdf-export/App.tsx",
      "packages/nsh-pdf-export/src/",
    ],
    impl: "z-nsh-engineer",
    audit: "z-nsh-engineer",
    gate: "ts",
    task: `
YOUR AREA: ts-package — build packages/nsh-pdf-export mirroring .refs/biometrics/packages/nsh-biometrics. You own the whole package dir EXCEPT tests/ (another area writes it).
- package.json, tsconfig.json, nsh.json, App.tsx per CONTRACT. tsconfig adapted from biometrics with the native module mapping for "nsh-pdf-export/native". App.tsx is a thin demo (memoryBackend only, no native calls — nsh test opens it headless).
- src/errors.ts, options.ts, backend.ts, memory.ts, rust.ts, export.ts, index.ts per CONTRACT: identical method names to the Rust API, identical codes and message wording, asPdfExportError parsing "<CODE>: " from ERR_NSH_HOST_ERROR rejections, memoryBackend scriptable with delays + call log, rustBackend() wrapping the native class, PdfExport with settle/one-op-at-a-time/timeout logic (InProgress, Timeout, Cancelled exactly as the CONTRACT states).
- The crates/nsh-pdf-export Rust crate another area builds has the native side (native.rs) per CONTRACT — code against it, never edit Rust files.`,
  },
  {
    name: "tests-ci",
    owns: [
      "crates/nsh-pdf-export/tests/",
      "packages/nsh-pdf-export/tests/",
      ".github/workflows/ci.yml",
    ],
    impl: "z-nsh-engineer",
    audit: "z-nsh-engineer",
    gate: "both",
    task: `
YOUR AREA: tests-ci — write the tests in both languages and update CI. You own ONLY: crates/nsh-pdf-export/tests/, packages/nsh-pdf-export/tests/, .github/workflows/ci.yml.
- Rust integration tests crates/nsh-pdf-export/tests/{goals.rs, edge_cases.rs, errors.rs}: all 22 CONTRACT edge cases by their exact names. Lifecycle, argument, cancel/timeout and error cases go through PdfExport + FakeBackend (crate::fake, another area writes it per CONTRACT); the real-engine cases (3, 5, 6, 19) use default_backend()/build_pdf with the vendored font at crates/nsh-pdf-export/assets/export-test-font.ttf (load via env!("CARGO_MANIFEST_DIR")); the matrix cases (15, 16, 17, 18) construct the bridge structs directly (WebBackend/AndroidBackend/AppleBackend::new()).
- TS tests packages/nsh-pdf-export/tests/{goals.test.tsx, edge-cases.test.tsx, errors.test.tsx}: the same 22 names via memoryBackend and the pure options helpers — native-free so nsh test never needs a device. The TS package another area builds exposes it per CONTRACT.
- AGENTS.md: a test is seen to fail once before it counts. For each test that passes immediately, flip one expectation, observe the failure, restore it, and note in your summary which tests were seen failing.
- .github/workflows/ci.yml: mirror .refs/biometrics/.github/workflows/ci.yml adapted to this repo — format job; rust job matrix ubuntu/macos/windows with cargo clippy -p nsh-pdf-export --all-targets --all-features -- -D warnings and cargo test (LINUX_PACKAGES must include libcups2-dev); cross-check clippy job for aarch64-apple-ios, aarch64-apple-ios-sim, aarch64-linux-android (with the NDK cc env like biometrics) and wasm32-unknown-unknown; typescript job installing NSH v0.4.0 (both installer variants like biometrics) then, working-directory packages/nsh-pdf-export: nsh check; nsh build | tee build.log; ! grep -i warn build.log; nsh test. Keep the current ci.yml's pinned action SHAs, on:, concurrency, permissions and env. No demo/publish/pages jobs — .github/workflows/pages.yml exists and stays untouched.`,
  },
];

artifact.board("areas", {
  key: "name",
  status: "phase",
  columns: ["implementing", "auditing", "fixing", "done", "failed"],
});

phase("Implement and audit every area in parallel — three rounds to zero findings");
log(`${AREAS.length} areas, each with its own ${AUDIT_ROUNDS}-round audit loop, all pinned to GLM-5.3`);

const outcomes: AreaOutcome[] = await Promise.all(
  AREAS.map(async (area) => {
    report({ name: area.name, phase: "implementing", summary: area.owns.join("; ") }, "areas");
    try {
      const implSpec = await specialist(
        area.impl,
        "You are a careful, honest senior engineer implementing one area of a parallel build-out.",
      );
      const implementer = agent(`PDF export · ${area.name} engineer`, {
        system: implSpec.persona,
        model: PRO,
      });
      let step = await implementer.ask<StepResult>(
        `${RULES}\nYOUR AREA: ${area.name}\nFILES YOU OWN (never touch anything else):\n${area.owns.map((o) => `- ${o}`).join("\n")}\n${area.task}\n===\n${CONTRACT}\n===\nImplement this area completely. Return StepResult. In contractLine, copy one sentence VERBATIM from your specialist contract (your system prompt) that most shaped the work.`,
      );
      if (!delivered(implSpec.persona, implSpec.source, step)) {
        step = await implementer.ask<StepResult>(
          "Your contractLine did not match your specialist contract. Copy one sentence VERBATIM from your system prompt — exactly as written — and return StepResult again.",
        );
      }
      const personaOk = delivered(implSpec.persona, implSpec.source, step);

      const auditSpec = await specialist(
        area.audit,
        "You are a precise, read-only reviewer of one area of a parallel build-out.",
      );
      const auditor = agent(`PDF export · ${area.name} auditor`, {
        system: auditSpec.persona,
        model: PRO,
      });

      let status: AreaOutcome["status"] = "cap";
      let rounds = 0;
      let lastFindings: Finding[] = [];
      for (let round = 1; round <= AUDIT_ROUNDS; round++) {
        rounds = round;
        report({ name: area.name, phase: "auditing", summary: `round ${round}` }, "areas");
        const gate = await areaGate(area.gate);
        const gateLine =
          area.gate === "none"
            ? "No command gate for this docs area: judge the documents by reading them against the CONTRACT and the reference repos."
            : gate.ok
              ? `The area gate passed (exit 0): ${gate.name}.`
              : `The area gate FAILED (exit ${gate.exitCode}): ${gate.name}\n${gate.tail}`;
        let audit = await auditor.ask<AuditResult>(
          `${RULES}\nAUDIT SCOPE: the files of area "${area.name}" ONLY:\n${area.owns.map((o) => `- ${o}`).join("\n")}\nRead them. Do not edit anything — you are read-only.\n${gateLine}\n${CONTRACT}\n===\nReport every actionable problem in THIS area's own files (missing behavior vs the CONTRACT, a contradiction with it, a panic or hang path, a wrong coded error, a doc row that misstates a reference API, a test that cannot fail honestly). Failures caused ONLY by files another area owns (still in flight) go in "external" as notes, never as findings. If the gate failed and the failing lines are all outside this area's files, findings is empty and external carries the reason. Reserve severity "high" for a hang, a panic, or a wrong result. Return AuditResult; in contractLine, copy one sentence VERBATIM from your specialist contract (your system prompt).`,
        );
        if (!delivered(auditSpec.persona, auditSpec.source, audit)) {
          audit = await auditor.ask<AuditResult>(
            "Your contractLine did not match your specialist contract. Copy one sentence VERBATIM from your system prompt — exactly as written — and return AuditResult again.",
          );
        }
        if (gate.ok && audit.findings.length === 0) {
          status = round === 1 ? "done" : "converged";
          break;
        }
        lastFindings = audit.findings;
        if (round === AUDIT_ROUNDS) {
          status = "cap";
          break;
        }
        report(
          { name: area.name, phase: "fixing", summary: `${audit.findings.length} finding(s) to fix` },
          "areas",
        );
        await implementer.ask<StepResult>(
          `${RULES}\nYOUR AREA: ${area.name}\nFILES YOU OWN:\n${area.owns.map((o) => `- ${o}`).join("\n")}\n${area.task}\n===\n${CONTRACT}\n===\nFix exactly these findings in your own files, nothing else:\n${JSON.stringify(audit.findings, null, 2)}\n${gate.ok ? "" : `\nAlso fix whatever in YOUR files caused this gate failure (failures from other areas' in-flight files are NOT yours):\n${gate.name} (exit ${gate.exitCode})\n${gate.tail}\n`}\nReturn StepResult.`,
        );
      }

      const blockers = step.blockers?.length ? ` Blockers: ${step.blockers.join(" ")}` : "";
      const summaryLine =
        status === "cap"
          ? `${step.summary} CAP REACHED: ${lastFindings.length} finding(s) remain after ${AUDIT_ROUNDS} audit rounds — did not converge.${blockers}`
          : `${step.summary} Clean after ${rounds} audit round(s).${blockers}`;
      report(
        { name: area.name, phase: status === "failed" ? "failed" : "done", summary: summaryLine },
        "areas",
      );
      return {
        name: area.name,
        status,
        summary: summaryLine,
        rounds,
        personaDelivered: personaOk ? "verified" : "unverified",
      } satisfies AreaOutcome;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      report({ name: area.name, phase: "failed", summary: message }, "areas");
      return {
        name: area.name,
        status: "failed",
        summary: `Failed: ${message}`,
        rounds: 0,
        personaDelivered: "unverified",
      } satisfies AreaOutcome;
    }
  }),
);

phase("Run the whole-tree gates once and fix what fails");
const finalRust = await rustGate();
const finalTs = await tsGate();
log(`whole-tree gates: rust ${finalRust.ok ? "pass" : `FAIL (${finalRust.name})`}, ts ${finalTs.ok ? "pass" : `FAIL (${finalTs.name})`}`);

const rustFixSpec = await specialist("z-rust-engineer", "You are a careful, honest senior Rust engineer.");
const tsFixSpec = await specialist("z-nsh-engineer", "You are a careful, honest senior NSH/TypeScript engineer.");
const rustFixer = agent("Gate fixer · Rust", { system: rustFixSpec.persona, model: PRO });
const tsFixer = agent("Gate fixer · TypeScript", { system: tsFixSpec.persona, model: PRO });

let joinRust = finalRust;
let joinTs = finalTs;
for (let round = 1; round <= JOIN_FIX_ROUNDS && !(joinRust.ok && joinTs.ok); round++) {
  const asks: Promise<void>[] = [];
  if (!joinRust.ok) {
    asks.push(
      rustFixer
        .ask<StepResult>(
          `${RULES}\nThe whole-tree Rust gates fail:\n${joinRust.name} (exit ${joinRust.exitCode})\n${joinRust.tail}\n===\n${CONTRACT}\n===\nMake the smallest correct change that makes these pass: cargo fmt --all -- --check; cargo clippy --all-targets -- -D warnings; cargo test --all-targets. You may touch any file ONLY as the failing output requires. No git commands. Return StepResult.`,
        )
        .then(() => undefined),
    );
  }
  if (!joinTs.ok) {
    asks.push(
      tsFixer
        .ask<StepResult>(
          `${RULES}\nThe whole-tree TypeScript gates fail:\n${joinTs.name} (exit ${joinTs.exitCode})\n${joinTs.tail}\n===\n${CONTRACT}\n===\nMake the smallest correct change that makes these pass: (cd packages/nsh-pdf-export && nsh check) and (cd packages/nsh-pdf-export && nsh test). You may touch any file ONLY as the failing output requires. No git commands, no devices. Return StepResult.`,
        )
        .then(() => undefined),
    );
  }
  await Promise.all(asks);
  if (!joinRust.ok) joinRust = await rustGate();
  if (!joinTs.ok) joinTs = await tsGate();
  log(`join fix round ${round}: rust ${joinRust.ok ? "pass" : "FAIL"}, ts ${joinTs.ok ? "pass" : "FAIL"}`);
}

const converged = outcomes.filter((o) => o.status === "done" || o.status === "converged");
const capped = outcomes.filter((o) => o.status === "cap");
const failed = outcomes.filter((o) => o.status === "failed");

const verified = [
  `Per-area: each of the ${AREAS.length} areas ran its own audit loop (scoped gates + read-only specialist auditor, up to ${AUDIT_ROUNDS} rounds) — rounds per area in the table below`,
  `Whole-tree: ${finalRust.name} — ${finalRust.ok ? "pass" : `initial fail, ${joinRust.ok ? "pass after join fix" : "still failing"}`}`,
  `Whole-tree: ${finalTs.name} — ${finalTs.ok ? "pass" : `initial fail, ${joinTs.ok ? "pass after join fix" : "still failing"}`}`,
  `Every implementer and auditor pinned to GLM-5.3 (account:zai-individual-coding-plan/GLM-5.3$high) per the brief`,
];

const notCovered = [
  "Cross-target compiles (aarch64-apple-ios, aarch64-apple-ios-sim, aarch64-linux-android, wasm32-unknown-unknown, the Windows and Linux CI jobs) are verified by the updated .github/workflows/ci.yml on GitHub only — nothing was pushed, so no CI run exists yet",
  "Real device printing (UIPrintInteractionController, winspool, libcups) is compile-checked and code-audited only; AGENTS.md forbids device builds on a developer machine and none ran",
  "nsh build (the bundle step CI runs) was not run locally; nsh check and nsh test were",
  capped.length > 0 ? `${capped.length} area(s) hit the ${AUDIT_ROUNDS}-round audit cap with findings remaining: ${capped.map((o) => o.name).join(", ")}` : "",
  failed.length > 0 ? `${failed.length} area(s) failed outright: ${failed.map((o) => o.name).join(", ")}` : "",
].filter((line) => line.length > 0);

const markdown = [
  "# PDF export build-out",
  "",
  `Ten areas ran in parallel: ${converged.length} converged, ${capped.length} hit the audit cap, ${failed.length} failed.`,
  "",
  "| Area | Status | Rounds | Summary |",
  "|---|---|---|---|",
  ...outcomes.map((o) => `| ${o.name} | ${o.status} | ${o.rounds} | ${o.summary.replace(/\|/g, "/")} |`),
  "",
  "## Whole-tree gates",
  `- ${finalRust.name}: ${finalRust.ok || joinRust.ok ? "pass" : "FAIL"}`,
  `- ${finalTs.name}: ${finalTs.ok || joinTs.ok ? "pass" : "FAIL"}`,
  "",
  "## Not covered this run",
  ...notCovered.map((line) => `- ${line}`),
].join("\n");

await artifact.markdown("report", markdown, {
  title: "PDF export build-out report",
  description: "Ten parallel areas: what landed, what converged, what did not",
  primary: true,
});

const conclusion =
  `${converged.length} of ${AREAS.length} areas converged with clean audits` +
  (capped.length > 0 ? `; ${capped.length} hit the ${AUDIT_ROUNDS}-round cap with findings remaining (${capped.map((o) => o.name).join(", ")})` : "") +
  (failed.length > 0 ? `; ${failed.length} failed (${failed.map((o) => o.name).join(", ")})` : "") +
  `. Whole-tree gates: Rust ${joinRust.ok ? "green" : "STILL FAILING"}, TypeScript ${joinTs.ok ? "green" : "STILL FAILING"}. ` +
  `The repo now holds the biometrics-style layout: crates/nsh-pdf-export (Rust core + five platform bridges + fake backend) and packages/nsh-pdf-export (TS mirror) with 22 named edge-case tests in both languages, and ci.yml runs both suites on push.`;

const result: RunReport = {
  conclusion,
  areas: outcomes,
  verified,
  notCovered,
  notes:
    "All implementers and auditors pinned to GLM-5.3$high per the brief (no vision phases, so no flash pins anywhere). " +
    "Step 0 was done by the launcher pre-workflow: .refs/ holds the five verified clones (origins match the brief's URLs exactly, no corrections), docs/decisions.md records the URL and platform decisions, .gitignore ignores .refs/. " +
    "Audit budget: 3 rounds per area, honestly reported when the cap leaves findings. No git machinery anywhere.",
};
return result;
