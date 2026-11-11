/** The text of one page, kept as the lines it was given. */
export interface PageText {
  lines: string[];
}

/** What a PDF is made of. v1 lays out plain text only, as `kind` says. */
export interface PdfSource {
  kind: "text";
  title: string;
  pages: PageText[];
}

/** A named page size, or one in millimetres. */
export type PageSize =
  | "a3"
  | "a4"
  | "a5"
  | "letter"
  | "legal"
  | "tabloid"
  | { widthMm: number; heightMm: number };

/** The paper a locale prints on: the US' letter, the rest of the world's a4. */
export function systemDefault(locale: string): PageSize {
  return locale === "en-US" ? "letter" : "a4";
}

/** A page size in millimetres, whatever form it was given in. */
export function sizeMm(size: PageSize): { widthMm: number; heightMm: number } {
  if (typeof size !== "string") return { widthMm: size.widthMm, heightMm: size.heightMm };
  const named: Record<Exclude<PageSize, object>, { widthMm: number; heightMm: number }> = {
    a3: { widthMm: 297, heightMm: 420 },
    a4: { widthMm: 210, heightMm: 297 },
    a5: { widthMm: 148, heightMm: 210 },
    letter: { widthMm: 215.9, heightMm: 279.4 },
    legal: { widthMm: 215.9, heightMm: 355.6 },
    tabloid: { widthMm: 279.4, heightMm: 431.8 },
  };
  return named[size];
}

/** The options of one export or save. Unset fields keep the platform's defaults. */
export interface ExportOptions {
  /** The paper each page is laid out on. */
  pageSize?: PageSize;
  /** The document's title, over the source's own. */
  title?: string;
}

/** The options of one HTML conversion. */
export interface HtmlOptions {
  /** The page's inner margin, in millimetres. */
  paddingMm?: number;
  /** A CSS color, as `#rrggbb`, behind the page's content. */
  backgroundColor?: string;
  /** The page's own address, so relative links and images resolve. */
  baseUrl?: string;
}

/** The options of one print job. */
export interface PrintOptions {
  /** The printer to use. The system's default when unset. */
  printerName?: string;
  /** The job's name, as the print queue shows it. */
  jobName?: string;
  /** How many copies to print, 1 to 999. The default is 1. */
  copies?: number;
}

/** A printer the system knows, as `listPrinters` reports it. */
export interface Printer {
  name: string;
  isDefault: boolean;
}

/** A print job the system accepted, by its id. */
export interface PrintJob {
  id: string;
}

/** What a platform can do, as `supports` reports it. */
export type Capability = "createPdf" | "saveToPath" | "saveDialog" | "convertHtml" | "print" | "listPrinters";
