//! The sources an export lays out, the paper it lays them out on, and each call's options.

/// The text of one page, kept as the lines it was given.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageText {
    /// The page's lines, top to bottom, laid out without re-wrapping.
    pub lines: Vec<String>,
}

/// What a [`PdfSource`] holds. v1 lays out plain text only.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PdfSourceKind {
    /// `lines` of plain text, one page per entry.
    #[default]
    Text,
}

/// What a PDF is made of.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PdfSource {
    /// What the source holds; v1 is text-only.
    pub kind: PdfSourceKind,
    /// The document's title, as a PDF reader shows it.
    pub title: String,
    /// The document's pages, in order. An export needs at least one.
    pub pages: Vec<PageText>,
}

impl PdfSource {
    /// The pages of one text page, titled `title`.
    pub fn text(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            kind: PdfSourceKind::Text,
            title: title.into(),
            pages: vec![PageText { lines }],
        }
    }
}

/// A named page size, or one given in millimetres.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// 297 x 420 mm.
    A3,
    /// 210 x 297 mm.
    A4,
    /// 148 x 210 mm.
    A5,
    /// 215.9 x 279.4 mm, the US letter.
    Letter,
    /// 215.9 x 355.6 mm, the US legal.
    Legal,
    /// 279.4 x 431.8 mm, the US tabloid.
    Tabloid,
    /// Any size, in millimetres; both sides must be positive and finite.
    Custom { width_mm: f64, height_mm: f64 },
}

/// A page's sides, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeMm {
    /// The page's width.
    pub width_mm: f64,
    /// The page's height.
    pub height_mm: f64,
}

/// The paper a locale prints on: the US' letter, the rest of the world's a4.
pub fn system_default(locale: &str) -> PageSize {
    match locale {
        "en-US" => PageSize::Letter,
        _ => PageSize::A4,
    }
}

/// A page size in millimetres, whatever form it was given in.
pub fn size_mm(page_size: &PageSize) -> SizeMm {
    let (width_mm, height_mm) = match *page_size {
        PageSize::A3 => (297.0, 420.0),
        PageSize::A4 => (210.0, 297.0),
        PageSize::A5 => (148.0, 210.0),
        PageSize::Letter => (215.9, 279.4),
        PageSize::Legal => (215.9, 355.6),
        PageSize::Tabloid => (279.4, 431.8),
        PageSize::Custom {
            width_mm,
            height_mm,
        } => (width_mm, height_mm),
    };
    SizeMm {
        width_mm,
        height_mm,
    }
}

/// The options of one export or save. Unset fields keep the platform's defaults.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportOptions {
    /// The paper each page is laid out on. The default is [`PageSize::A4`].
    pub page_size: Option<PageSize>,
    /// The document's title, over the source's own.
    pub title: Option<String>,
}

/// The options of one HTML conversion. Platforms with an HTML engine read these; v1 has none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HtmlOptions {
    /// The page's inner margin, in millimetres.
    pub padding_mm: Option<f64>,
    /// A CSS color, as `#rrggbb`, behind the page's content.
    pub background_color: Option<String>,
    /// The page's own address, so relative links and images resolve.
    pub base_url: Option<String>,
}

/// The options of one print job.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrintOptions {
    /// The printer to use. The system's default when unset.
    pub printer_name: Option<String>,
    /// The job's name, as the print queue shows it.
    pub job_name: Option<String>,
    /// How many copies to print, 1 to 999. The default is 1.
    pub copies: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_locale_picks_letter_only_for_the_us() {
        assert_eq!(system_default("en-US"), PageSize::Letter);
        for locale in ["en-GB", "de-DE", "fr", "ja-JP", "", "en-us"] {
            assert_eq!(system_default(locale), PageSize::A4, "locale {locale:?}");
        }
    }

    #[test]
    fn every_named_size_has_its_sides() {
        let sizes = [
            (PageSize::A3, 297.0, 420.0),
            (PageSize::A4, 210.0, 297.0),
            (PageSize::A5, 148.0, 210.0),
            (PageSize::Letter, 215.9, 279.4),
            (PageSize::Legal, 215.9, 355.6),
            (PageSize::Tabloid, 279.4, 431.8),
        ];
        for (page_size, width_mm, height_mm) in sizes {
            let size = size_mm(&page_size);
            assert_eq!(size.width_mm, width_mm, "the width of {page_size:?}");
            assert_eq!(size.height_mm, height_mm, "the height of {page_size:?}");
        }
    }

    #[test]
    fn a_custom_size_passes_through_unchanged() {
        let size = size_mm(&PageSize::Custom {
            width_mm: 100.5,
            height_mm: 200.25,
        });
        assert_eq!(
            size,
            SizeMm {
                width_mm: 100.5,
                height_mm: 200.25
            }
        );
    }

    #[test]
    fn a_text_source_starts_with_one_page() {
        let source = PdfSource::text("Report", vec!["Hello PDF".to_owned()]);
        assert_eq!(source.kind, PdfSourceKind::Text);
        assert_eq!(source.title, "Report");
        assert_eq!(source.pages.len(), 1);
        assert_eq!(source.pages[0].lines, vec!["Hello PDF".to_owned()]);
    }
}
