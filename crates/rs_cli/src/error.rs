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

    #[allow(dead_code)]
    #[error("unknown or undetectable format: {0}")]
    UnknownFormat(String),

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
