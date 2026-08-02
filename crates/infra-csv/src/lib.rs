//! Scrat CSV import adapter: delimiter/column-type detection and mapping of
//! raw bank export rows into date/amount/description triples for the
//! application layer to turn into transactions.

mod detection;

pub use detection::{
    build_preview, decode_bytes, detect_columns, detect_header, parse_amount_cell, parse_date_cell,
    sniff_delimiter, ColumnDetection, ImportPreview, ParsedRow,
};
