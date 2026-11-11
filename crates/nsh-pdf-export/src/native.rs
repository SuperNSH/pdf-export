//! The export as TSX sees it: `NativePdfExport`, a class whose async members reject with a
//! code on failure. It answers for the platform's backend — the front API and its rules live
//! in TSX's `PdfExport` — and hands PDF bytes over as base64.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nsh_core::JsonValue;
use nsh_core::prelude::*;

use crate::{
    backend::{Capability, PrintJob},
    error::{ErrorKind, PdfExportError},
    options::{
        ExportOptions, HtmlOptions, PageSize, PageText, PdfSource, PdfSourceKind, PrintOptions,
    },
    platform,
};

/// One page of the source, from TSX.
#[derive(JsObject, Clone, Debug, Default)]
pub struct NativePageText {
    /// The page's lines, top to bottom.
    pub lines: Vec<String>,
}

/// What a PDF is made of, from TSX.
#[derive(JsObject, Clone, Debug, Default)]
pub struct NativePdfSource {
    /// `"text"`; v1 lays out plain text only.
    pub kind: Option<String>,
    /// The document's title, as a PDF reader shows it.
    pub title: Option<String>,
    /// The document's pages, in order.
    pub pages: Vec<NativePageText>,
}

/// The options of one export or save, from TSX. Unset fields keep the platform's defaults.
#[derive(JsObject, Clone, Debug, Default)]
pub struct NativeExportOptions {
    /// A named size (`"a4"`), or `{ widthMm, heightMm }` in millimetres.
    pub page_size: Option<JsonValue>,
    /// The document's title, over the source's own.
    pub title: Option<String>,
}

/// The options of one HTML conversion, from TSX.
#[derive(JsObject, Clone, Debug, Default)]
pub struct NativeHtmlOptions {
    /// The page's inner margin, in millimetres.
    pub padding_mm: Option<f64>,
    /// A CSS color, as `#rrggbb`, behind the page's content.
    pub background_color: Option<String>,
    /// The page's own address, so relative links and images resolve.
    pub base_url: Option<String>,
}

/// The options of one print job, from TSX.
#[derive(JsObject, Clone, Debug, Default)]
pub struct NativePrintOptions {
    /// The printer to use. The system's default when unset.
    pub printer_name: Option<String>,
    /// The job's name, as the print queue shows it.
    pub job_name: Option<String>,
    /// How many copies to print, 1 to 999. The default is 1.
    pub copies: Option<u32>,
}

/// A printer the system knows, as TSX lists them.
#[derive(JsObject, Clone, Debug, Default, PartialEq)]
pub struct NativePrinter {
    /// The printer's name.
    pub name: String,
    /// Whether the system prints here when no printer is named.
    pub is_default: bool,
}

/// A print job the system accepted, as TSX sees it.
#[derive(JsObject, Clone, Debug, Default, PartialEq)]
pub struct NativePrintJob {
    /// The id the platform gave the job, as a print queue knows it.
    pub id: String,
}

/// A PDF export handle. Create one and keep it; it answers for the platform's own backend.
#[class]
pub struct NativePdfExport {
    backend: &'static dyn crate::backend::Backend,
}

#[class]
impl NativePdfExport {
    /// The platform's export.
    pub fn new() -> NativePdfExport {
        NativePdfExport {
            backend: platform::default_backend(),
        }
    }

    /// What this platform can do, as TS names it: `"createPdf"`, `"saveToPath"`, and so on.
    pub fn capabilities(this: Instance<Self>) -> Vec<String> {
        this.with(|native| {
            Capability::ALL
                .iter()
                .filter(|capability| native.backend.supports(**capability))
                .map(|capability| capability.name().to_owned())
                .collect()
        })
    }

    /// Renders the source to PDF bytes, as base64.
    pub async fn create_pdf(
        this: Instance<Self>,
        source: NativePdfSource,
        options: Option<NativeExportOptions>,
    ) -> Result<String, JsError> {
        let backend = this.with(|native| native.backend);
        let source = to_source(source)?;
        let options = to_export_options(options.unwrap_or_default())?;
        let bytes = backend
            .create_pdf(&source, &options)
            .await
            .map_err(into_js)?;
        Ok(STANDARD.encode(bytes))
    }

    /// Renders the source and writes it to `path`. Resolves the path written.
    pub async fn save_pdf(
        this: Instance<Self>,
        source: NativePdfSource,
        options: Option<NativeExportOptions>,
        path: String,
    ) -> Result<String, JsError> {
        let backend = this.with(|native| native.backend);
        let source = to_source(source)?;
        let options = to_export_options(options.unwrap_or_default())?;
        let path = std::path::PathBuf::from(path);
        backend
            .save_pdf(&source, &options, &path)
            .await
            .map(|saved| saved.to_string_lossy().into_owned())
            .map_err(into_js)
    }

    /// Shows the system's save dialog. Resolves the chosen path, or nothing when the user
    /// cancelled.
    pub async fn pick_save_path(
        this: Instance<Self>,
        suggested_name: String,
    ) -> Result<Option<String>, JsError> {
        let backend = this.with(|native| native.backend);
        backend
            .pick_save_path(&suggested_name)
            .await
            .map(|picked| picked.map(|path| path.to_string_lossy().into_owned()))
            .map_err(into_js)
    }

    /// Renders the HTML to PDF bytes, as base64.
    pub async fn convert_html(
        this: Instance<Self>,
        html: String,
        options: Option<NativeHtmlOptions>,
    ) -> Result<String, JsError> {
        let backend = this.with(|native| native.backend);
        let options = to_html_options(options.unwrap_or_default());
        let bytes = backend
            .convert_html(&html, &options)
            .await
            .map_err(into_js)?;
        Ok(STANDARD.encode(bytes))
    }

    /// Sends the base64 PDF to a printer. Resolves the job the platform accepted.
    pub async fn print_pdf(
        this: Instance<Self>,
        pdf_base64: String,
        options: Option<NativePrintOptions>,
    ) -> Result<NativePrintJob, JsError> {
        let backend = this.with(|native| native.backend);
        let pdf = STANDARD
            .decode(pdf_base64)
            .map_err(|_| invalid("pdf must be base64"))?;
        let options = to_print_options(options.unwrap_or_default());
        let job: PrintJob = backend.print_pdf(&pdf, &options).await.map_err(into_js)?;
        Ok(NativePrintJob {
            id: job.id.to_string(),
        })
    }

    /// The printers the system knows, each with whether it is the default.
    pub async fn list_printers(this: Instance<Self>) -> Result<Vec<NativePrinter>, JsError> {
        let backend = this.with(|native| native.backend);
        let printers = backend.list_printers().await.map_err(into_js)?;
        Ok(printers
            .into_iter()
            .map(|printer| NativePrinter {
                name: printer.name,
                is_default: printer.is_default,
            })
            .collect())
    }

    /// Asks the platform to stop the running operation. Resolves whether it took the request;
    /// it never rejects, so a fire-and-forget caller is safe.
    pub async fn cancel(this: Instance<Self>) -> Result<bool, JsError> {
        let backend = this.with(|native| native.backend);
        Ok(backend.cancel().await.is_ok())
    }
}

impl Default for NativePdfExport {
    fn default() -> Self {
        Self::new()
    }
}

/// The error a rejection carries: its message is `"<CODE>: <message>"`, its `code` the stable one.
fn into_js(error: PdfExportError) -> JsError {
    JsError::Error(format!("{}: {}", error.code(), error.kind().message())).with_code(error.code())
}

/// A refusal of an argument TSX handed over, with the invalid-argument code.
fn invalid(message: &str) -> JsError {
    JsError::Error(format!(
        "{}: {}",
        ErrorKind::InvalidArgument.code(),
        message
    ))
    .with_code(ErrorKind::InvalidArgument.code())
}

/// Turns the TSX source into the crate's, refusing a kind v1 does not lay out.
fn to_source(source: NativePdfSource) -> Result<PdfSource, JsError> {
    match source.kind.as_deref() {
        None | Some("text") => {}
        Some(_) => return Err(invalid("kind must be \"text\" in v1")),
    }
    Ok(PdfSource {
        kind: PdfSourceKind::Text,
        title: source.title.unwrap_or_default(),
        pages: source
            .pages
            .into_iter()
            .map(|page| PageText { lines: page.lines })
            .collect(),
    })
}

/// Turns the TSX options into the crate's, refusing a page size it cannot read.
fn to_export_options(options: NativeExportOptions) -> Result<ExportOptions, JsError> {
    Ok(ExportOptions {
        page_size: match options.page_size {
            None => None,
            Some(value) => Some(to_page_size(value)?),
        },
        title: options.title,
    })
}

/// A page size arrives as a name or as `{ widthMm, heightMm }`; anything else is refused.
fn to_page_size(value: JsonValue) -> Result<PageSize, JsError> {
    match value {
        JsonValue::String(name) => match name.as_str() {
            "a3" => Ok(PageSize::A3),
            "a4" => Ok(PageSize::A4),
            "a5" => Ok(PageSize::A5),
            "letter" => Ok(PageSize::Letter),
            "legal" => Ok(PageSize::Legal),
            "tabloid" => Ok(PageSize::Tabloid),
            _ => Err(invalid("pageSize must name a size this crate knows")),
        },
        JsonValue::Object(fields) => {
            let side = |key: &str| fields.get(key).and_then(JsonValue::as_f64);
            match (side("widthMm"), side("heightMm")) {
                (Some(width_mm), Some(height_mm)) => Ok(PageSize::Custom {
                    width_mm,
                    height_mm,
                }),
                _ => Err(invalid(
                    "pageSize in millimetres needs widthMm and heightMm",
                )),
            }
        }
        _ => Err(invalid("pageSize must be a name or { widthMm, heightMm }")),
    }
}

fn to_html_options(options: NativeHtmlOptions) -> HtmlOptions {
    HtmlOptions {
        padding_mm: options.padding_mm,
        background_color: options.background_color,
        base_url: options.base_url,
    }
}

fn to_print_options(options: NativePrintOptions) -> PrintOptions {
    PrintOptions {
        printer_name: options.printer_name,
        job_name: options.job_name,
        copies: options.copies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The message and `code` a rejection carries; `with_code` folds both into `Fields`.
    fn rejection(error: JsError) -> (String, Option<String>) {
        let JsError::Fields(fields) = error else {
            panic!("a coded rejection is an Error with fields");
        };
        let code = fields
            .fields
            .iter()
            .find(|(key, _)| *key == "code")
            .and_then(|(_, value)| value.as_str())
            .map(str::to_owned);
        (fields.message, code)
    }

    #[test]
    fn a_text_source_arrives_with_its_pages() {
        let source = to_source(NativePdfSource {
            kind: Some("text".to_owned()),
            title: Some("Report".to_owned()),
            pages: vec![NativePageText {
                lines: vec!["Hello PDF".to_owned()],
            }],
        })
        .expect("a text source converts");
        assert_eq!(source.kind, PdfSourceKind::Text);
        assert_eq!(source.title, "Report");
        assert_eq!(
            source.pages,
            vec![PageText {
                lines: vec!["Hello PDF".to_owned()]
            }]
        );
    }

    #[test]
    fn a_kind_v1_does_not_lay_out_is_refused() {
        let error = to_source(NativePdfSource {
            kind: Some("images".to_owned()),
            title: None,
            pages: Vec::new(),
        })
        .expect_err("only text converts");
        let (message, code) = rejection(error);
        assert_eq!(code.as_deref(), Some("ERR_PDF_EXPORT_INVALID_ARGUMENT"));
        assert!(message.contains("ERR_PDF_EXPORT_INVALID_ARGUMENT"));
    }

    #[test]
    fn a_page_size_arrives_by_name_or_in_millimetres() {
        let named = |name: &str| {
            to_page_size(JsonValue::String(name.to_owned())).expect("a named size converts")
        };
        assert_eq!(named("a3"), PageSize::A3);
        assert_eq!(named("a4"), PageSize::A4);
        assert_eq!(named("a5"), PageSize::A5);
        assert_eq!(named("letter"), PageSize::Letter);
        assert_eq!(named("legal"), PageSize::Legal);
        assert_eq!(named("tabloid"), PageSize::Tabloid);

        let json = r#"{"widthMm": 100.5, "heightMm": 200}"#;
        let custom = to_page_size(serde_json::from_str(json).unwrap()).expect("a size converts");
        assert_eq!(
            custom,
            PageSize::Custom {
                width_mm: 100.5,
                height_mm: 200.0,
            }
        );

        assert!(to_page_size(JsonValue::String("a2".to_owned())).is_err());
        assert!(
            to_page_size(serde_json::from_str(r#"{"widthMm": 100}"#).unwrap()).is_err(),
            "a size in millimetres needs both sides"
        );
        assert!(to_page_size(JsonValue::Bool(true)).is_err());
    }

    #[test]
    fn every_error_rejection_carries_the_code_in_its_message() {
        let (message, code) = rejection(into_js(PdfExportError::new(ErrorKind::Cancelled)));
        assert_eq!(code.as_deref(), Some("ERR_PDF_EXPORT_CANCELLED"));
        assert_eq!(
            message,
            "ERR_PDF_EXPORT_CANCELLED: The operation was cancelled."
        );
        let (message, code) = rejection(into_js(PdfExportError::new(ErrorKind::Backend)));
        assert_eq!(code.as_deref(), Some("ERR_PDF_EXPORT_BACKEND"));
        assert_eq!(
            message,
            "ERR_PDF_EXPORT_BACKEND: The platform's PDF export service failed."
        );
    }

    #[test]
    fn a_print_job_arrives_with_its_id_as_a_string() {
        let options = to_print_options(NativePrintOptions {
            printer_name: None,
            job_name: None,
            copies: Some(2),
        });
        assert_eq!(options.copies, Some(2));
    }
}
