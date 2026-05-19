//! Engine — factory for creating and opening documents.

use std::path::Path;

use crate::document::Document;
use crate::error::Error;
use crate::format::Format;

/// The main entry point for s1engine.
///
/// `Engine` is a lightweight factory for creating and opening documents.
/// It holds no state and can be shared across threads.
///
/// # Example
///
/// ```no_run
/// use s1engine::Engine;
///
/// let engine = Engine::new();
/// let doc = engine.create();
/// ```
pub struct Engine;

impl Engine {
    /// Create a new engine instance.
    pub fn new() -> Self {
        Self
    }

    /// Create a new empty document.
    pub fn create(&self) -> Document {
        Document::new()
    }

    /// Open a document from raw bytes.
    ///
    /// The format is auto-detected from the content.
    pub fn open(&self, data: &[u8]) -> Result<Document, Error> {
        let format = Format::detect(data);
        self.open_as(data, format)
    }

    /// Open a document from raw bytes with an explicit format.
    pub fn open_as(&self, data: &[u8], format: Format) -> Result<Document, Error> {
        // DOCX takes the preservation-aware path so `export(Docx)` can
        // round-trip the file losslessly when no edits happen.
        #[cfg(feature = "docx")]
        if matches!(format, Format::Docx) {
            let (model, pkg) = s1_format_docx::reader::read_with_package(data)?;
            return Ok(Document::from_model_with_package(model, pkg));
        }

        let model = match format {
            #[cfg(feature = "docx")]
            Format::Docx => s1_format_docx::read(data)?,
            #[cfg(feature = "odt")]
            Format::Odt => s1_format_odt::read(data)?,
            #[cfg(feature = "txt")]
            Format::Txt => {
                let result = s1_format_txt::read(data)?;
                result.document
            }
            #[cfg(feature = "md")]
            Format::Md => {
                s1_format_md::read_bytes(data).map_err(|e| Error::Format(e.to_string()))?
            }
            #[cfg(feature = "convert")]
            Format::Doc => {
                s1_convert::doc_reader::read_doc(data).map_err(|e| Error::Format(e.to_string()))?
            }
            #[cfg(feature = "convert")]
            Format::Csv => {
                s1_convert::csv_to_model(data).map_err(|e| Error::Format(e.to_string()))?
            }
            #[allow(unreachable_patterns)]
            _ => {
                return Err(Error::UnsupportedFormat(format!(
                    "{:?} reading not available (check feature flags)",
                    format
                )));
            }
        };
        Ok(Document::from_model(model))
    }

    /// Open a document from a file path.
    ///
    /// Format is detected from the file extension.
    pub fn open_file(&self, path: impl AsRef<Path>) -> Result<Document, Error> {
        let path = path.as_ref();
        let format = Format::from_path(path)?;
        let data = std::fs::read(path)?;
        self.open_as(&data, format)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
