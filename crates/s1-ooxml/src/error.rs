//! Error type for `s1-ooxml`.

use thiserror::Error;

/// Convenience alias.
pub type Result<T> = std::result::Result<T, OoxmlError>;

/// Anything that can go wrong while reading or writing an OOXML package.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OoxmlError {
    /// The underlying ZIP container is broken.
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// I/O failure while reading or writing.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// XML parsing failed for a specific part.
    #[error("xml error in part `{part}`: {source}")]
    Xml {
        /// Name of the part that failed to parse.
        part: String,
        /// Underlying quick-xml error.
        #[source]
        source: quick_xml::Error,
    },

    /// A required part was not found in the package.
    #[error("missing part: {0}")]
    MissingPart(String),

    /// The package is malformed in a way we can't recover from.
    #[error("malformed package: {0}")]
    Malformed(String),

    /// UTF-8 decoding failed when reading an XML part.
    #[error("utf-8 decoding failed in part `{part}`: {source}")]
    Utf8 {
        /// Part name.
        part: String,
        /// Underlying error.
        #[source]
        source: std::string::FromUtf8Error,
    },
}
