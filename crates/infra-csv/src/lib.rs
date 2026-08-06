//! Scrat CSV import adapter.
//!
//! Two layers, kept apart on purpose:
//!
//! - [`detection`] — decoding, delimiter sniffing, and per-cell/per-column
//!   type parsing. Knows nothing about what a column *means*.
//! - [`mapping`] — guesses a [`ColumnMapping`] from a file, and separately
//!   reads a file through one. Detection is heuristic over adversarial
//!   input and will sometimes be wrong; the import dialog lets the user
//!   correct the mapping, and the corrected one is read back through the
//!   same [`apply_mapping`] every detected mapping goes through.

mod detection;
mod mapping;
mod operation_kind;

pub use detection::{
    DATE_FORMATS, decode_bytes, detect_date_format, detect_header, parse_amount_cell,
    parse_date_cell, parse_date_cell_with, parse_rows, sniff_delimiter,
};
pub use mapping::{
    AmountSource, ColumnMapping, ColumnSummary, ImportPreview, ParsedFile, ParsedRow,
    apply_mapping, build_preview, detect_mapping, file_signature, parse_file, preview_with_mapping,
    summarize_columns,
};
