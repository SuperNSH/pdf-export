# PDF export: brief

Creating PDFs from app content, and printing where the platform allows.

## Options

- printpdf: pure Rust PDF creation
- UIPrintInteraction: Apple printing, native

## Recommended path

printpdf for creating files. Native printing on Apple.

## Edge cases to design for

- Font embedding must be verified with the app's fonts
- Large documents use a lot of memory; stream pages
- Page size varies by locale

## Research status

Research done 2026-11-09 by web search. Versions and licences are unverified until checked on the registry before pinning. Full catalogue: the research notes for the SuperNSH library set.
