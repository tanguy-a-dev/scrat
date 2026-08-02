use chrono::NaiveDate;

const CANDIDATE_DELIMITERS: [char; 4] = [',', ';', '\t', '|'];

/// Formats tried in priority order — when a date string is ambiguous under
/// more than one format (e.g. "01/07/2026" is valid as both D/M/Y and
/// M/D/Y whenever the day is ≤ 12), the earlier-listed format wins.
const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d", "%d/%m/%Y", "%m/%d/%Y", "%d-%m-%Y", "%d.%m.%Y", "%Y/%m/%d",
];

/// Strips a UTF-8 BOM if present, then decodes as UTF-8; falls back to
/// Windows-1252 (a common legacy encoding for European bank exports) if the
/// bytes aren't valid UTF-8.
pub fn decode_bytes(bytes: &[u8]) -> String {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Picks the delimiter whose row-length distribution is most consistent
/// across the first ~20 non-blank lines.
pub fn sniff_delimiter(text: &str) -> char {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    if lines.is_empty() {
        return ',';
    }

    CANDIDATE_DELIMITERS
        .into_iter()
        .max_by(|&a, &b| {
            consistency_score(&lines, a)
                .partial_cmp(&consistency_score(&lines, b))
                .unwrap()
        })
        .unwrap_or(',')
}

// Weighted by the modal field count itself, not just how consistently it
// recurs — a decimal separator (comma or dot) can look like a perfectly
// consistent "delimiter" that happens to split every amount into exactly
// two fields, but the real structural delimiter almost always produces
// more columns than that, so this keeps a lone decimal point from
// outscoring the actual field separator.
fn consistency_score(lines: &[&str], delimiter: char) -> f64 {
    let field_counts: Vec<usize> = lines
        .iter()
        .map(|l| l.matches(delimiter).count() + 1)
        .collect();
    if field_counts.iter().all(|&c| c <= 1) {
        return 0.0;
    }
    let mut frequency = std::collections::HashMap::new();
    for &count in &field_counts {
        *frequency.entry(count).or_insert(0) += 1;
    }
    let (&modal_count, &modal_frequency) = frequency.iter().max_by_key(|&(_, freq)| freq).unwrap();
    let consistency = modal_frequency as f64 / field_counts.len() as f64;
    consistency * modal_count as f64
}

/// Parses `text` into rows of raw string cells using `delimiter`. Tolerates
/// ragged rows (a real bank export commonly has a couple of summary lines
/// with fewer columns than the transaction rows).
pub fn parse_rows(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter as u8)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    reader
        .records()
        .filter_map(|r| r.ok())
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect()
}

/// Parses a cell as a date, trying each of [`DATE_FORMATS`] in order.
pub fn parse_date_cell(raw: &str) -> Option<NaiveDate> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    DATE_FORMATS
        .iter()
        .find_map(|fmt| NaiveDate::parse_from_str(trimmed, fmt).ok())
}

/// Parses a cell as a signed amount in integer minor units (cents). Handles
/// both `,` and `.` as the decimal separator: whichever of the two appears
/// *last* in the string is the decimal point if exactly 1–2 digits follow
/// it; otherwise every separator is treated as a thousands grouping.
pub fn parse_amount_cell(raw: &str) -> Option<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !matches!(c, '€' | '$' | '£' | ' ' | '\u{a0}' | '+'))
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let is_negative = cleaned.starts_with('-');

    let digits_and_seps: String = cleaned
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    if digits_and_seps.is_empty() {
        return None;
    }

    let last_comma = digits_and_seps.rfind(',');
    let last_dot = digits_and_seps.rfind('.');
    let decimal_pos = match (last_comma, last_dot) {
        (Some(c), Some(d)) => Some(c.max(d)),
        (Some(c), None) => Some(c),
        (None, Some(d)) => Some(d),
        (None, None) => None,
    };

    let (integer_digits, fraction_digits) = match decimal_pos {
        Some(pos) => {
            let trailing: String = digits_and_seps[pos + 1..]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if trailing.len() == 1 || trailing.len() == 2 {
                let leading: String = digits_and_seps[..pos]
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                (leading, trailing)
            } else {
                // Not a plausible decimal point (e.g. a 3-digit thousands
                // grouping) — every separator is a thousands grouping.
                let all_digits: String = digits_and_seps
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                (all_digits, String::new())
            }
        }
        None => (
            digits_and_seps
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect(),
            String::new(),
        ),
    };

    if integer_digits.is_empty() && fraction_digits.is_empty() {
        return None;
    }
    // A column of long reference/account numbers can still fully parse as
    // digits — reject implausibly large amounts outright rather than
    // silently coercing an unparseable one to 0 (which previously let such
    // a column masquerade as a perfectly valid, if wrong, amount column)
    // or overflowing i64 on the `* 100` below.
    const MAX_PLAUSIBLE_INTEGER: i64 = 999_999_999;
    let integer_value: i64 = integer_digits.parse().ok()?;
    if integer_value > MAX_PLAUSIBLE_INTEGER {
        return None;
    }
    let fraction_value: i64 = match fraction_digits.len() {
        0 => 0,
        1 => fraction_digits.parse::<i64>().unwrap_or(0) * 10,
        _ => fraction_digits[..2].parse().unwrap_or(0),
    };
    let minor_units = integer_value
        .checked_mul(100)?
        .checked_add(fraction_value)?;
    Some(if is_negative {
        -minor_units
    } else {
        minor_units
    })
}

/// Does the first row look like a header? True only if every column that
/// otherwise looks like a date or amount column has a first-row cell that
/// fails to parse the same way the rest of the column does.
pub fn detect_header(rows: &[Vec<String>]) -> bool {
    if rows.len() < 2 {
        return false;
    }
    let first = &rows[0];
    let rest = &rows[1..];
    let column_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);

    let mut type_bearing_columns = 0;
    let mut header_like_columns = 0;

    for col in 0..column_count {
        let rest_values: Vec<&str> = rest
            .iter()
            .filter_map(|r| r.get(col))
            .map(|s| s.as_str())
            .filter(|s| !s.trim().is_empty())
            .collect();
        if rest_values.is_empty() {
            continue;
        }
        let date_rate = rest_values
            .iter()
            .filter(|v| parse_date_cell(v).is_some())
            .count() as f64
            / rest_values.len() as f64;
        let amount_rate = rest_values
            .iter()
            .filter(|v| parse_amount_cell(v).is_some())
            .count() as f64
            / rest_values.len() as f64;

        if date_rate < 0.8 && amount_rate < 0.8 {
            continue;
        }
        type_bearing_columns += 1;

        let first_cell = first.get(col).map(|s| s.as_str()).unwrap_or("");
        let first_parses = if date_rate >= 0.8 {
            parse_date_cell(first_cell).is_some()
        } else {
            parse_amount_cell(first_cell).is_some()
        };
        if !first_parses {
            header_like_columns += 1;
        }
    }

    type_bearing_columns > 0 && header_like_columns == type_bearing_columns
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnDetection {
    pub date_column: usize,
    pub amount_column: usize,
    pub date_score: f64,
    pub amount_score: f64,
}

/// Scores every column by what fraction of its (non-blank) values parse as
/// a date, and separately as an amount, then assigns Date to the
/// highest-scoring column and Amount to the best of the *remaining*
/// columns.
pub fn detect_columns(rows: &[Vec<String>]) -> Option<ColumnDetection> {
    let column_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if column_count == 0 {
        return None;
    }

    let mut date_scores = vec![0.0; column_count];
    let mut amount_scores = vec![0.0; column_count];

    for (col, (date_score, amount_score)) in date_scores
        .iter_mut()
        .zip(amount_scores.iter_mut())
        .enumerate()
    {
        let values: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.get(col))
            .map(|s| s.as_str())
            .filter(|s| !s.trim().is_empty())
            .collect();
        if values.is_empty() {
            continue;
        }
        // Weight the hit-rate by how much of the column is even populated —
        // without this, a column that's blank in all but one row (a
        // sparse type/flag column, say) can score a coincidental 100% on a
        // single sample and out-rank the real Date/Amount column, which is
        // populated in nearly every row but not always 100% clean.
        let coverage = values.len() as f64 / rows.len() as f64;
        let date_hits = values
            .iter()
            .filter(|v| parse_date_cell(v).is_some())
            .count();
        let amount_hits = values
            .iter()
            .filter(|v| parse_amount_cell(v).is_some())
            .count();
        *date_score = (date_hits as f64 / values.len() as f64) * coverage;
        *amount_score = (amount_hits as f64 / values.len() as f64) * coverage;
    }

    let date_column =
        (0..column_count).max_by(|&a, &b| date_scores[a].partial_cmp(&date_scores[b]).unwrap())?;
    let amount_column = (0..column_count)
        .filter(|&c| c != date_column)
        .max_by(|&a, &b| amount_scores[a].partial_cmp(&amount_scores[b]).unwrap())?;

    Some(ColumnDetection {
        date_column,
        amount_column,
        date_score: date_scores[date_column],
        amount_score: amount_scores[amount_column],
    })
}

#[derive(Debug, Clone)]
pub struct ParsedRow {
    pub date: Option<NaiveDate>,
    pub amount_minor_units: Option<i64>,
    pub description: String,
    /// The raw text of a header column named "Category"/"Catégorie", if the
    /// file has a header row and one exists — used to file the row under a
    /// matching category (creating it if needed) instead of the fallback
    /// category chosen for the whole import; see `commit_csv_import`.
    pub csv_category: Option<String>,
    /// The raw text of a header column named "Subcategory"/"Sous-catégorie",
    /// if the file has a header row and one exists — nests under
    /// `csv_category` when both are applied (see `commit_csv_import`),
    /// mirroring the app's own CSV export format.
    pub csv_subcategory: Option<String>,
    /// True when this row looks like a bank's opening/closing balance line
    /// rather than a real transaction — see `is_boundary_balance_row`.
    /// Doesn't affect `is_valid` (the row still parses as a perfectly good
    /// date+amount); it only changes the default checked state the import
    /// UI starts the row at, since the heuristic can misfire and the user
    /// can always re-check the row.
    pub is_likely_balance_row: bool,
    pub raw: Vec<String>,
}

impl ParsedRow {
    /// A row usable for import needs both a date and a non-zero amount —
    /// `Transaction` itself rejects zero amounts, and a missing date can't
    /// be defaulted sensibly.
    pub fn is_valid(&self) -> bool {
        self.date.is_some() && matches!(self.amount_minor_units, Some(a) if a != 0)
    }
}

/// The most common field count across `rows` — the shape real transaction
/// rows share, used by [`is_boundary_balance_row`] as the baseline a
/// boundary row is compared against.
fn modal_row_len(rows: &[Vec<String>]) -> usize {
    let mut frequency = std::collections::HashMap::new();
    for r in rows {
        *frequency.entry(r.len()).or_insert(0) += 1;
    }
    frequency
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(len, _)| len)
        .unwrap_or(0)
}

/// True if row `idx` is the file's first or last data row and is
/// structurally thinner than the rest. Most banks bookend an export with
/// one row per boundary carrying the balance as of the start/end date — just
/// a date, an amount, and a reference/account code — missing the
/// transaction-type, counterparty, and reference columns a real transaction
/// row has, so it parses into fewer fields than the modal row.
///
/// Only the very first and very last row are ever considered: a short row
/// in the middle of the file is far more likely to be a genuine transaction
/// with some blank trailing fields than a balance line, and banks only ever
/// bookend the whole file, never the middle of it.
fn is_boundary_balance_row(rows: &[Vec<String>], idx: usize, modal_len: usize) -> bool {
    rows.len() >= 3 && (idx == 0 || idx == rows.len() - 1) && rows[idx].len() < modal_len
}

#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub rows: Vec<ParsedRow>,
    pub date_confidence: f64,
    pub amount_confidence: f64,
}

/// Finds the first header cell (skipping `exclude`, if given) whose
/// lowercased text contains any of `keywords` — the general substring match
/// every specific-column finder below is built on, since bank exports label
/// columns inconsistently (language, casing, punctuation).
fn find_labeled_column(
    header: &[String],
    keywords: &[&str],
    exclude: Option<usize>,
) -> Option<usize> {
    header.iter().enumerate().find_map(|(i, cell)| {
        if Some(i) == exclude {
            return None;
        }
        let lower = cell.trim().to_lowercase();
        keywords.iter().any(|k| lower.contains(k)).then_some(i)
    })
}

/// Finds a header cell naming the subcategory column, e.g. "Subcategory" or
/// the French "Sous-catégorie" — checked *before* the category column since
/// "subcategory" also contains "categ" and would otherwise be mistaken for it.
fn find_subcategory_column(header: &[String]) -> Option<usize> {
    find_labeled_column(
        header,
        &["subcateg", "sous-categ", "sous categ", "souscateg"],
        None,
    )
}

/// Finds a header cell naming the category column, e.g. "Category" or the
/// French "Catégorie" — matched loosely (substring, case-insensitive, and
/// tolerant of the accent) since bank exports label it inconsistently.
/// Excludes `subcategory_column` since "Subcategory" also matches "categ".
fn find_category_column(header: &[String], subcategory_column: Option<usize>) -> Option<usize> {
    find_labeled_column(header, &["categ", "catég"], subcategory_column)
}

/// Finds a header cell naming the currency column, e.g. "Currency" or the
/// French "Devise" — the app has a single global currency setting (see
/// `settings.currency_code`), so a per-row currency cell is never applied to
/// the transaction, only excluded from `description` so it doesn't pollute it.
fn find_currency_column(header: &[String]) -> Option<usize> {
    find_labeled_column(header, &["currency", "devise"], None)
}

/// Finds a header cell naming the (bank) account column, e.g. "Account" or
/// the French "Compte" — the destination account is chosen once for the
/// whole import (or defaulted), so a per-row account cell is only excluded
/// from `description`, never used to pick the account.
fn find_account_column(header: &[String]) -> Option<usize> {
    find_labeled_column(header, &["account", "compte"], None)
}

/// The full detection pipeline: decode → sniff delimiter → parse → detect
/// (and drop) a header → detect Date/Amount columns → build one
/// [`ParsedRow`] per line, concatenating every other non-empty column as
/// `description` (real exports often shift the description between columns
/// depending on transaction type, so picking a single "description column"
/// isn't reliable — concatenating whatever's left is). Columns identified by
/// header name as Category, Subcategory, Currency, or Account are excluded
/// from that concatenation: Category/Subcategory are surfaced separately
/// (`csv_category`/`csv_subcategory`), and Currency/Account never belong in
/// `description` (the app has one global currency, and the destination account is
/// chosen once for the whole import) — otherwise they'd leak into it, e.g.
/// "EUR" and a bank name getting prepended/appended to every description. Each
/// row is also checked against [`is_boundary_balance_row`] and flagged via
/// `is_likely_balance_row` when it looks like an opening/closing balance
/// line rather than a real transaction.
pub fn build_preview(bytes: &[u8]) -> ImportPreview {
    let text = decode_bytes(bytes);
    let delimiter = sniff_delimiter(&text);
    let mut rows = parse_rows(&text, delimiter);

    let (category_column, subcategory_column, currency_column, account_column) =
        if detect_header(&rows) && !rows.is_empty() {
            let header = rows.remove(0);
            let subcategory_column = find_subcategory_column(&header);
            let category_column = find_category_column(&header, subcategory_column);
            let currency_column = find_currency_column(&header);
            let account_column = find_account_column(&header);
            (
                category_column,
                subcategory_column,
                currency_column,
                account_column,
            )
        } else {
            (None, None, None, None)
        };

    let Some(detection) = detect_columns(&rows) else {
        return ImportPreview {
            rows: Vec::new(),
            date_confidence: 0.0,
            amount_confidence: 0.0,
        };
    };

    let modal_len = modal_row_len(&rows);
    let parsed_rows = rows
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let date_cell = raw
                .get(detection.date_column)
                .map(|s| s.as_str())
                .unwrap_or("");
            let amount_cell = raw
                .get(detection.amount_column)
                .map(|s| s.as_str())
                .unwrap_or("");
            let cell_text = |col: Option<usize>| {
                col.and_then(|c| raw.get(c))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            };
            let csv_category = cell_text(category_column);
            let csv_subcategory = cell_text(subcategory_column);
            let description = raw
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    let i = Some(*i);
                    i != Some(detection.date_column)
                        && i != Some(detection.amount_column)
                        && i != category_column
                        && i != subcategory_column
                        && i != currency_column
                        && i != account_column
                })
                .map(|(_, v)| v.trim())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join(" ");

            ParsedRow {
                date: parse_date_cell(date_cell),
                amount_minor_units: parse_amount_cell(amount_cell),
                description,
                csv_category,
                csv_subcategory,
                is_likely_balance_row: is_boundary_balance_row(&rows, i, modal_len),
                raw: raw.clone(),
            }
        })
        .collect();

    ImportPreview {
        rows: parsed_rows,
        date_confidence: detection.date_score,
        amount_confidence: detection.amount_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_delimiter_prefers_semicolon_for_semicolon_separated_data() {
        let text = "01/07/2026;10,00;Carte;;STORE A\n02/07/2026;-5,00;Carte;;STORE B\n";
        assert_eq!(sniff_delimiter(text), ';');
    }

    #[test]
    fn sniff_delimiter_tolerates_ragged_summary_rows() {
        // Two 4-field "balance" rows plus several 8-field transaction rows —
        // modeled on a real bank export shape (never real account data).
        let text = "\
01/01/2026;1000,00;;ACC REF
01/01/2026;-20,00;Carte;;STORE A;;0;Misc
02/01/2026;-30,00;Carte;;STORE B;;0;Misc
03/01/2026;50,00;Virement;;;WAGES;;
05/01/2026;999,00;;ACC REF
";
        assert_eq!(sniff_delimiter(text), ';');
    }

    #[test]
    fn parse_date_cell_parses_day_first_format() {
        assert_eq!(
            parse_date_cell("01/07/2026"),
            NaiveDate::from_ymd_opt(2026, 7, 1)
        );
    }

    #[test]
    fn parse_date_cell_parses_iso_format() {
        assert_eq!(
            parse_date_cell("2026-07-01"),
            NaiveDate::from_ymd_opt(2026, 7, 1)
        );
    }

    #[test]
    fn parse_date_cell_rejects_garbage() {
        assert_eq!(parse_date_cell("not a date"), None);
    }

    #[test]
    fn parse_amount_cell_handles_comma_decimal() {
        assert_eq!(parse_amount_cell("841,76"), Some(84_176));
    }

    #[test]
    fn parse_amount_cell_handles_negative_whole_number() {
        assert_eq!(parse_amount_cell("-35"), Some(-3_500));
    }

    #[test]
    fn parse_amount_cell_handles_single_trailing_digit() {
        assert_eq!(parse_amount_cell("2291,1"), Some(229_110));
    }

    #[test]
    fn parse_amount_cell_handles_dot_decimal() {
        assert_eq!(parse_amount_cell("841.76"), Some(84_176));
    }

    #[test]
    fn parse_amount_cell_handles_thousands_grouping_with_decimal() {
        assert_eq!(parse_amount_cell("1.234,56"), Some(123_456));
        assert_eq!(parse_amount_cell("1,234.56"), Some(123_456));
    }

    #[test]
    fn parse_amount_cell_rejects_garbage() {
        assert_eq!(parse_amount_cell("n/a"), None);
    }

    #[test]
    fn parse_amount_cell_rejects_implausibly_large_reference_number_without_panicking() {
        // A long account/reference number can still parse as a plain
        // integer (no separators) — this must not crash, and must not be
        // mistaken for a real amount.
        assert_eq!(parse_amount_cell("00878020105123456789"), None);
    }

    #[test]
    fn parse_amount_cell_does_not_overflow_on_near_i64_max_integer() {
        // A value whose integer part alone fits i64 but overflows once
        // multiplied by 100 (minor units) must be rejected, not panic.
        assert_eq!(parse_amount_cell("123456789012345678"), None);
    }

    #[test]
    fn detect_header_returns_false_for_headerless_data() {
        let rows = parse_rows(
            "01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n03/07/2026;-6,00;STORE C\n",
            ';',
        );
        assert!(!detect_header(&rows));
    }

    #[test]
    fn detect_header_returns_true_when_first_row_is_labels() {
        let rows = parse_rows(
            "Date;Amount;Description\n01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n",
            ';',
        );
        assert!(detect_header(&rows));
    }

    #[test]
    fn detect_columns_identifies_date_and_amount() {
        let rows = parse_rows(
            "01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n03/07/2026;-6,00;STORE C\n",
            ';',
        );
        let detection = detect_columns(&rows).unwrap();
        assert_eq!(detection.date_column, 0);
        assert_eq!(detection.amount_column, 1);
        assert!(detection.date_score > 0.8);
        assert!(detection.amount_score > 0.8);
    }

    #[test]
    fn modal_row_len_picks_the_most_common_field_count() {
        let rows = parse_rows(
            "01/07/2026;500,00;;REF\n01/07/2026;-35,00;Carte;;STORE;;0;Misc\n01/07/2026;10,00;Carte;;STORE;;0;Misc\n",
            ';',
        );
        assert_eq!(modal_row_len(&rows), 8);
    }

    #[test]
    fn is_boundary_balance_row_flags_only_a_thinner_first_or_last_row() {
        // Three modal-shape (8-field) rows outnumber the two 4-field
        // boundary rows, so `modal_len` is unambiguous.
        let rows = parse_rows(
            "01/07/2026;500,00;;REF\n01/07/2026;-35,00;Carte;;STORE;;0;Misc\n01/07/2026;10,00;Carte;;STORE;;0;Misc\n01/07/2026;-1,00;Carte;;STORE;;0;Misc\n05/07/2026;600,00;;REF\n",
            ';',
        );
        let modal_len = modal_row_len(&rows);
        assert!(is_boundary_balance_row(&rows, 0, modal_len));
        assert!(is_boundary_balance_row(&rows, 4, modal_len));
        assert!(!is_boundary_balance_row(&rows, 1, modal_len));
        assert!(!is_boundary_balance_row(&rows, 2, modal_len));
        assert!(!is_boundary_balance_row(&rows, 3, modal_len));
    }

    #[test]
    fn build_preview_on_headerless_ragged_data_concatenates_remaining_columns_as_description() {
        // Structurally mirrors a real French bank export: no header,
        // semicolon-delimited, decimal comma, two 4-field balance rows
        // bookending several 8-field transaction rows whose description
        // shifts column depending on transaction type. All values here are
        // fabricated.
        let text = "\
01/07/2026;500,00;;ACC REF 12345
01/07/2026;-35,00;Carte;;CB SOME STORE   30/06/26;;0;Divers
01/07/2026;120,50;Virement;;;INCOMING WAGES;;
01/07/2026;-19,99;Virement;;PRLV SEPA SOME BILL;;;
05/07/2026;565,51;;ACC REF 12345
";
        let preview = build_preview(text.as_bytes());

        assert!(preview.date_confidence > 0.8);
        assert!(preview.amount_confidence > 0.8);
        assert_eq!(preview.rows.len(), 5);

        let card_row = &preview.rows[1];
        assert_eq!(card_row.date, NaiveDate::from_ymd_opt(2026, 7, 1));
        assert_eq!(card_row.amount_minor_units, Some(-3_500));
        assert!(card_row.description.contains("CB SOME STORE"));

        let wire_row = &preview.rows[2];
        assert_eq!(wire_row.amount_minor_units, Some(12_050));
        assert!(wire_row.description.contains("INCOMING WAGES"));

        // The two summary/balance rows still parse as date+amount, and stay
        // `is_valid` — this is exactly why the import UI keeps a per-row
        // include/exclude checkbox rather than silently dropping rows. But
        // they're structurally thinner (4 fields) than the surrounding
        // transaction rows (8 fields) and sit at the file's boundary, so
        // they get flagged as likely balance lines and default unchecked.
        assert!(preview.rows[0].is_valid());
        assert!(preview.rows[0].is_likely_balance_row);
        assert!(preview.rows[4].is_valid());
        assert!(preview.rows[4].is_likely_balance_row);

        assert!(!card_row.is_likely_balance_row);
        assert!(!wire_row.is_likely_balance_row);
    }

    #[test]
    fn build_preview_does_not_flag_a_short_row_in_the_middle_of_the_file() {
        // A short row that isn't at the file's boundary is far more likely
        // to be a genuine transaction with some blank trailing fields (e.g.
        // no counterparty reference) than a balance line — only the first
        // and last row are ever candidates.
        let text = "\
01/07/2026;-35,00;Carte;;CB SOME STORE   30/06/26;;0;Divers
02/07/2026;10,00;;
03/07/2026;-19,99;Virement;;PRLV SEPA SOME BILL;;;
";
        let preview = build_preview(text.as_bytes());
        assert_eq!(preview.rows.len(), 3);
        assert!(!preview.rows[1].is_likely_balance_row);
    }

    #[test]
    fn build_preview_does_not_flag_boundary_rows_in_a_very_short_file() {
        // With fewer than 3 rows there's no reliable "modal" shape to
        // compare against, so nothing is flagged even if the two rows
        // differ in length.
        let text = "\
01/07/2026;500,00;;ACC REF 12345
01/07/2026;-35,00;Carte;;CB SOME STORE   30/06/26;;0;Divers
";
        let preview = build_preview(text.as_bytes());
        assert_eq!(preview.rows.len(), 2);
        assert!(preview.rows.iter().all(|r| !r.is_likely_balance_row));
    }

    #[test]
    fn build_preview_extracts_csv_category_column_and_excludes_it_from_description() {
        let text = "\
Date;Amount;Description;Category
04/04/2023;-60,80;SC-SUSHI SASHI;Food & Drinks
04/04/2023;-20,97;LES SUPER HEROS;Books
";
        let preview = build_preview(text.as_bytes());

        assert_eq!(preview.rows.len(), 2);
        assert_eq!(
            preview.rows[0].csv_category,
            Some("Food & Drinks".to_string())
        );
        assert_eq!(preview.rows[0].description, "SC-SUSHI SASHI");
        assert_eq!(preview.rows[1].csv_category, Some("Books".to_string()));
        assert_eq!(preview.rows[1].description, "LES SUPER HEROS");
    }

    #[test]
    fn build_preview_has_no_csv_category_without_a_matching_header() {
        let text = "01/07/2026;10,00;STORE A\n02/07/2026;-5,00;STORE B\n";
        let preview = build_preview(text.as_bytes());

        assert!(preview.rows.iter().all(|r| r.csv_category.is_none()));
    }

    #[test]
    fn build_preview_extracts_subcategory_and_excludes_currency_and_account_from_description() {
        // Mirrors the app's own export format (Date;Amount;Currency;Description;
        // Category;Subcategory;Account) — Currency and Account must not leak
        // into `description`, and Subcategory must be captured separately from
        // Category rather than folded into the description concatenation.
        let text = "\
Date;Amount;Currency;Description;Category;Subcategory;Account
2026-08-04;-12,25;EUR;HEMA GARERER CHA;Home;;LCL
2026-08-04;-20,97;EUR;LES SUPER HEROS;Education;Books;LCL
2026-08-04;-60,80;EUR;SC-SUSHI SASHI;Food & Drinks;;LCL
";
        let preview = build_preview(text.as_bytes());

        assert_eq!(preview.rows.len(), 3);

        assert_eq!(preview.rows[0].description, "HEMA GARERER CHA");
        assert_eq!(preview.rows[0].csv_category, Some("Home".to_string()));
        assert_eq!(preview.rows[0].csv_subcategory, None);

        assert_eq!(preview.rows[1].description, "LES SUPER HEROS");
        assert_eq!(preview.rows[1].csv_category, Some("Education".to_string()));
        assert_eq!(preview.rows[1].csv_subcategory, Some("Books".to_string()));

        assert_eq!(preview.rows[2].description, "SC-SUSHI SASHI");
        assert_eq!(
            preview.rows[2].csv_category,
            Some("Food & Drinks".to_string())
        );
    }

    #[test]
    fn build_preview_decodes_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"01/07/2026;10,00;STORE A\n");
        let preview = build_preview(&bytes);
        assert_eq!(preview.rows.len(), 1);
        assert_eq!(preview.rows[0].date, NaiveDate::from_ymd_opt(2026, 7, 1));
    }
}
