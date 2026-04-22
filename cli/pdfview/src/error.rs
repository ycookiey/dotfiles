use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum PdfViewError {
    #[error("PDF file not found: {0}")]
    FileNotFound(PathBuf),
    #[error(
        "pdfium library not found. Place pdfium.dll next to pdfview.exe or install via `scoop install yscoopy/pdfview`.\nUnderlying error: {0}"
    )]
    PdfiumInit(String),
    #[error("Password-protected PDFs are not supported")]
    PasswordProtected,
    #[error("Failed to render page {page}: {source}")]
    RenderFailed {
        page: u32,
        #[source]
        source: anyhow::Error,
    },
    #[error("Render cancelled (superseded by newer request)")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl PdfViewError {
    /// CLI exit code matching the plan's expectations.
    pub fn exit_code(&self) -> i32 {
        match self {
            PdfViewError::FileNotFound(_) => 1,
            PdfViewError::PdfiumInit(_) => 1,
            PdfViewError::PasswordProtected => 2,
            PdfViewError::RenderFailed { .. } => 3,
            PdfViewError::Cancelled => 0,
            PdfViewError::Io(_) => 1,
            PdfViewError::Other(_) => 1,
        }
    }
}

pub type PdfViewResult<T> = Result<T, PdfViewError>;

/// Heuristic pdfium-render error classification. The crate exposes a
/// `PdfiumError` enum whose variants we match on by stringifying, since
/// we prefer not to leak pdfium_render types across our boundary.
pub fn classify_load_error(err: &anyhow::Error) -> Option<PdfViewError> {
    let msg = format!("{err:#}").to_ascii_lowercase();
    if msg.contains("password") {
        return Some(PdfViewError::PasswordProtected);
    }
    None
}
