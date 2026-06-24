#![forbid(unsafe_code)]
/*!
The CLI's single error type. Every library `Error` and `io::Error` converts into `CliError`
through `From`, and `main` renders it with miette's fancy report. Library code never prints;
this is the only place errors become human output.
*/

use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum CliError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("unknown or undetectable format: {0}")]
    UnknownFormat(String),

    #[error("format error")]
    Format(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[allow(dead_code)]
    #[error("unsupported conversion from .{from} to .{to}")]
    UnsupportedConversion { from: String, to: String },

    #[allow(dead_code)]
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = core::result::Result<T, CliError>;

impl CliError {
    #[allow(dead_code)]
    pub fn msg(text: impl Into<String>) -> Self {
        CliError::Message(text.into())
    }
}

impl From<ritoshark::bin::Error> for CliError {
    fn from(e: ritoshark::bin::Error) -> Self {
        CliError::Format(Box::new(e))
    }
}

impl From<ritoshark::wad::Error> for CliError {
    fn from(e: ritoshark::wad::Error) -> Self {
        CliError::Format(Box::new(e))
    }
}

impl From<ritoshark::tex::Error> for CliError {
    fn from(e: ritoshark::tex::Error) -> Self {
        CliError::Format(Box::new(e))
    }
}

impl From<ritoshark::rst::Error> for CliError {
    fn from(e: ritoshark::rst::Error) -> Self {
        CliError::Format(Box::new(e))
    }
}
