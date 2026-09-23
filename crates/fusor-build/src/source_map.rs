//! Shared, versioned source-map format for the compiler and diagnostic consumers.

use crate::BindingLocation;
use std::{error::Error, fmt, str::FromStr};

const HEADER: &str = "fusor-source-map-v1";

/// Ordered mappings from generated Rust line ranges to authored HTML bindings.
///
/// Ranges are one-based and end-exclusive. A binding location identifies the
/// originating attribute or text interpolation; this is not a token-level map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    locations: Vec<BindingLocation>,
}

impl SourceMap {
    /// Validate mappings produced by a compiler or another tool.
    pub fn new(locations: Vec<BindingLocation>) -> Result<Self, SourceMapError> {
        let mut previous_end = 1;
        for (index, entry) in locations.iter().enumerate() {
            if entry.generated_start < previous_end
                || entry.generated_end <= entry.generated_start
                || entry.line == 0
                || entry.column == 0
            {
                return Err(SourceMapError {
                    line: index + 2,
                    message: "ranges must be ordered, nonoverlapping, nonempty, and one-based",
                });
            }
            previous_end = entry.generated_end;
        }
        Ok(Self { locations })
    }

    /// Find the authored binding for a generated Rust line.
    pub fn lookup(&self, line: usize) -> Option<&BindingLocation> {
        let index = self
            .locations
            .partition_point(|entry| entry.generated_end <= line);
        self.locations
            .get(index)
            .filter(|entry| entry.generated_start <= line)
    }

    /// Inspect the validated mappings in generated-source order.
    pub fn locations(&self) -> &[BindingLocation] {
        &self.locations
    }
}

impl fmt::Display for SourceMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "{HEADER}")?;
        for entry in &self.locations {
            writeln!(
                formatter,
                "{}\t{}\t{}\t{}",
                entry.generated_start, entry.generated_end, entry.line, entry.column
            )?;
        }
        Ok(())
    }
}

/// Invalid or unsupported source-map data. Corrupt entries are never skipped.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceMapError {
    /// One-based line in the serialized source map.
    pub line: usize,
    message: &'static str,
}

impl fmt::Display for SourceMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid fusor source map at line {}: {}",
            self.line, self.message
        )
    }
}

impl Error for SourceMapError {}

impl FromStr for SourceMap {
    type Err = SourceMapError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err(SourceMapError {
                line: 1,
                message: "missing or unsupported format version; rebuild the application",
            });
        }
        let mut entries = Vec::new();
        for (index, line) in lines.enumerate() {
            let invalid = || SourceMapError {
                line: index + 2,
                message: "expected four tab-separated, nonnegative integers",
            };
            let fields = line
                .split('\t')
                .map(str::parse::<usize>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid())?;
            let [generated_start, generated_end, source_line, column] = fields.as_slice() else {
                return Err(invalid());
            };
            entries.push(BindingLocation {
                generated_start: *generated_start,
                generated_end: *generated_end,
                line: *source_line,
                column: *column,
            });
        }
        Self::new(entries)
    }
}
