use crate::{ExtractError, location};
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// A problem in one application source, shown as `path[:line:column]: message`.
#[derive(Debug)]
pub(super) struct SourceError {
    path: PathBuf,
    location: Option<(usize, usize)>,
    message: String,
}

impl SourceError {
    pub(super) fn new(path: &Path, message: impl fmt::Display) -> Self {
        Self {
            path: path.to_owned(),
            location: None,
            message: message.to_string(),
        }
    }

    pub(super) fn at(path: &Path, line: usize, column: usize, message: impl fmt::Display) -> Self {
        Self {
            location: Some((line, column)),
            ..Self::new(path, message)
        }
    }

    pub(super) fn at_offset(
        path: &Path,
        source: &str,
        offset: usize,
        message: impl fmt::Display,
    ) -> Self {
        let (line, column) = location(source, offset);
        Self::at(path, line, column, message)
    }

    pub(super) fn extracted(path: &Path, error: ExtractError) -> Self {
        Self::at(path, error.line, error.column, error.message)
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.path.display())?;
        if let Some((line, column)) = self.location {
            write!(formatter, ":{line}:{column}")?;
        }
        write!(formatter, ": {}", self.message)
    }
}

impl std::error::Error for SourceError {}
