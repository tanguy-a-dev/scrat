//! Primitives the mapping detector is built out of: decoding, delimiter
//! sniffing, row splitting, and per-cell/per-column type parsing.
//!
//! Nothing here decides *which* column means what — that's
//! [`crate::mapping`]. Keeping the two apart is what lets a user-corrected
//! mapping be applied through exactly the same code path as a detected one.

use chrono::NaiveDate;

const CANDIDATE_DELIMITERS: [char; 4] = [',', ';', '\t', '|'];

/// Date formats tried in priority order, each with a label for the import
/// dialog's format picker.
///
/// The order matters: when a string is ambiguous under more than one format
/// (`01/07/2026` is valid as both D/M/Y and M/D/Y whenever the day is ≤ 12),
/// the earlier entry wins. Day-first precedes month-first because this app's
/// users export from European banks — and because a whole column is
/// disambiguated by [`detect_date_format`] as soon as a single row has a day
/// above 12, which most months' worth of transactions will.
pub const DATE_FORMATS: &[(&str, &str)] = &[
    ("%Y-%m-%d", "2026-01-31  (YYYY-MM-DD)"),
    ("%d/%m/%Y", "31/01/2026  (DD/MM/YYYY)"),
    ("%m/%d/%Y", "01/31/2026  (MM/DD/YYYY)"),
    ("%d-%m-%Y", "31-01-2026  (DD-MM-YYYY)"),
    ("%d.%m.%Y", "31.01.2026  (DD.MM.YYYY)"),
    ("%Y/%m/%d", "2026/01/31  (YYYY/MM/DD)"),
];

/// Lowercases and strips the accents a French export routinely carries
/// ("Prélèvement", "Catégorie", "Débit"), so every keyword list in this crate
/// can be written in plain ASCII and still match. Shared with
/// [`crate::operation_kind`], which folds free text the same way.
pub(crate) fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ñ' => 'n',
            'ÿ' => 'y',
            other => other,
        })
        .collect()
}

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

/// Parses a cell as a date under one specific chrono format.
pub fn parse_date_cell_with(raw: &str, format: &str) -> Option<NaiveDate> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(trimmed, format).ok()
}

/// Parses a cell as a date, trying each of [`DATE_FORMATS`] in order. Used
/// for *detection* (does this column look like dates at all); once a column
/// is chosen, [`detect_date_format`] pins one format for the whole column so
/// a file can't silently switch interpretation halfway down.
pub fn parse_date_cell(raw: &str) -> Option<NaiveDate> {
    DATE_FORMATS
        .iter()
        .find_map(|(fmt, _)| parse_date_cell_with(raw, fmt))
}

/// Picks the one format that parses the most of `values`, falling back to
/// [`DATE_FORMATS`] order on a tie.
///
/// This is what resolves D/M/Y vs M/D/Y for a real column rather than a
/// single cell: `03/04/2026` alone is undecidable, but one `25/12/2026`
/// anywhere in the column rules month-first out entirely. Only when *every*
/// value is ambiguous does the tie-break (day-first) decide — and that's the
/// case the import dialog lets the user override by hand.
pub fn detect_date_format(values: &[&str]) -> &'static str {
    DATE_FORMATS
        .iter()
        .map(|(fmt, _)| {
            let hits = values
                .iter()
                .filter(|v| parse_date_cell_with(v, fmt).is_some())
                .count();
            (*fmt, hits)
        })
        .fold(("%Y-%m-%d", 0), |best, current| {
            if current.1 > best.1 {
                current
            } else {
                best
            }
        })
        .0
}

/// Currency codes allowed to sit next to an amount. Anything else alphabetic
/// in the cell means it isn't one — see [`parse_amount_cell`].
const CURRENCY_CODES: &[&str] = &[
    "eur", "usd", "gbp", "chf", "cad", "aud", "jpy", "sek", "nok", "dkk", "pln", "czk",
];

/// Removes a leading or trailing currency code (`12,50 EUR`, `EUR 12,50`),
/// which is the only alphabetic text a genuine amount cell ever carries.
///
/// Sliced through `str::get` rather than by raw byte index: this is external
/// input, the codes are ASCII but the cell around them need not be, and a
/// slice landing inside a multi-byte character would panic — taking the whole
/// import down with it.
fn strip_currency_code(text: &str) -> &str {
    for code in CURRENCY_CODES {
        if text
            .get(..code.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(code))
        {
            let rest = &text[code.len()..];
            if rest.is_empty() || rest.starts_with(|c: char| !c.is_alphanumeric()) {
                return rest.trim();
            }
        }
        let Some(start) = text.len().checked_sub(code.len()) else {
            continue;
        };
        if text
            .get(start..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(code))
        {
            let rest = &text[..start];
            if rest.ends_with(|c: char| !c.is_alphanumeric()) {
                return rest.trim();
            }
        }
    }
    text
}

/// Parses a cell as a signed amount in integer minor units (cents). Handles
/// both `,` and `.` as the decimal separator: whichever of the two appears
/// *last* in the string is the decimal point if exactly 1–2 digits follow
/// it; otherwise every separator is treated as a thousands grouping.
///
/// **A cell containing letters is not an amount.** This used to strip every
/// non-digit character and parse whatever survived, so a description cell
/// like `CB PLACEMINUTE COM 30/06/26` "parsed" as 300 626 — enough for a
/// column of free text to score as a plausible amount column and win
/// detection outright. Currency codes are the one exception, and only where
/// a real amount puts them.
pub fn parse_amount_cell(raw: &str) -> Option<i64> {
    let trimmed = strip_currency_code(raw.trim());
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().any(char::is_alphabetic) {
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
        let rest_values = column_values(rest, col);
        if rest_values.is_empty() {
            continue;
        }
        let date_rate = rate(&rest_values, |v| parse_date_cell(v).is_some());
        let amount_rate = rate(&rest_values, |v| parse_amount_cell(v).is_some());

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

/// Every non-blank value in column `col`, trimmed of nothing (callers that
/// care trim themselves — the parsers all do).
pub(crate) fn column_values(rows: &[Vec<String>], col: usize) -> Vec<&str> {
    rows.iter()
        .filter_map(|r| r.get(col))
        .map(|s| s.as_str())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn rate(values: &[&str], predicate: impl Fn(&str) -> bool) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().filter(|v| predicate(v)).count() as f64 / values.len() as f64
}

/// What one column looks like, measured once and reused by every field's
/// detector rather than recomputed per candidate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ColumnStats {
    /// Fraction of this column's *non-blank* cells that parse as a date.
    pub date_rate: f64,
    /// Fraction of this column's *non-blank* cells that parse as an amount.
    pub amount_rate: f64,
    /// Fraction of *all* rows where this column is non-blank. Without this,
    /// a column that's blank in all but one row (a sparse type/flag column,
    /// say) can score a coincidental 100% on a single sample and out-rank
    /// the real Date/Amount column, which is populated in nearly every row
    /// but not always 100% clean.
    pub coverage: f64,
    /// See [`amount_plausibility`].
    pub amount_plausibility: f64,
}

/// How much a column that *parses* as numbers actually looks like money.
///
/// Parsing is not plausibility, and the gap between them is where this
/// crate's worst bug lived: Caisse d'Épargne exports a "Pointage operation"
/// column that is `0` on every row. It parses perfectly, is populated on
/// every row, and so scored a flawless 1.0 — beating the real "Debit"
/// column, which is blank on income rows and therefore scores lower. Every
/// amount in the file came out as zero, and since `Transaction` rejects zero
/// amounts, *nothing in the file was importable* while the UI reported 100%
/// confidence.
///
/// Two signals separate money from the other numeric columns a bank export
/// carries:
///
/// - **All zeroes.** Nothing that is zero on every row it appears in is a
///   ledger of amounts, and `Transaction` rejects zero amounts anyway.
///   Decisive at any sample size.
/// - **Variance.** More generally, a column whose non-blank values are all
///   the same number is a flag or a counter. Decisive too, but only once
///   there are enough samples to mean anything: the *credit* half of a
///   debit/credit pair legitimately holds a single value in a statement with
///   one income row, and "all one sample is identical to itself" must not
///   disqualify it.
/// - **Cents and signs.** Money is written `-9,99` or `+2521,14`; reference
///   numbers, counters and account codes are bare non-negative integers.
///   This one is suggestive rather than decisive — plenty of legitimate
///   exports round to whole units — so it only halves the score.
pub(crate) fn amount_plausibility(values: &[&str]) -> f64 {
    /// Below this many samples, "every value is identical" carries no
    /// information about the column.
    const MIN_SAMPLES_FOR_VARIANCE: usize = 3;

    if values.is_empty() {
        return 0.0;
    }
    let parsed: Vec<i64> = values.iter().filter_map(|v| parse_amount_cell(v)).collect();
    if parsed.is_empty() || parsed.iter().all(|&v| v == 0) {
        return 0.0;
    }
    let first = parsed[0];
    if parsed.len() >= MIN_SAMPLES_FOR_VARIANCE && parsed.iter().all(|&v| v == first) {
        return 0.0;
    }
    let expressive = rate(values, |v| {
        let t = v.trim();
        t.contains(',') || t.contains('.') || t.starts_with('-') || t.starts_with('+')
    });
    0.5 + 0.5 * expressive
}

/// Measures every column of `rows` once. `column_count` is passed in rather
/// than derived so ragged rows can't shrink the grid the mapping is
/// expressed against.
pub(crate) fn measure_columns(rows: &[Vec<String>], column_count: usize) -> Vec<ColumnStats> {
    (0..column_count)
        .map(|col| {
            let values = column_values(rows, col);
            if values.is_empty() || rows.is_empty() {
                return ColumnStats {
                    date_rate: 0.0,
                    amount_rate: 0.0,
                    coverage: 0.0,
                    amount_plausibility: 0.0,
                };
            }
            ColumnStats {
                date_rate: rate(&values, |v| parse_date_cell(v).is_some()),
                amount_rate: rate(&values, |v| parse_amount_cell(v).is_some()),
                coverage: values.len() as f64 / rows.len() as f64,
                amount_plausibility: amount_plausibility(&values),
            }
        })
        .collect()
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

    /// A single day above 12 rules month-first out for the whole column,
    /// which is the only evidence that exists for the D/M/Y vs M/D/Y
    /// question short of asking the user.
    #[test]
    fn detect_date_format_uses_an_unambiguous_row_to_settle_the_whole_column() {
        assert_eq!(
            detect_date_format(&["03/04/2026", "25/12/2026", "01/02/2026"]),
            "%d/%m/%Y"
        );
        assert_eq!(
            detect_date_format(&["03/04/2026", "12/25/2026", "01/02/2026"]),
            "%m/%d/%Y"
        );
    }

    /// When every value is ambiguous there is nothing to go on, so the
    /// listed order decides — and the import dialog lets the user override
    /// it, because this is exactly the case detection cannot win.
    #[test]
    fn detect_date_format_falls_back_to_day_first_when_every_value_is_ambiguous() {
        assert_eq!(
            detect_date_format(&["03/04/2026", "01/02/2026"]),
            "%d/%m/%Y"
        );
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

    /// The bug that let a whole column of free text masquerade as amounts:
    /// stripping non-digits left a parseable number behind in every one of
    /// these, and a description column then out-scored the real amount
    /// column.
    #[test]
    fn parse_amount_cell_rejects_text_that_merely_contains_digits() {
        for cell in [
            "CB  PLACEMINUTE COM  30/06/26",
            "INTERETS DEBITEURS AU 30 06 26",
            "00122 021115Z",
            "VIR SEPA ACME 2621122K10552598",
            "2621122G10408668-",
        ] {
            assert_eq!(parse_amount_cell(cell), None, "cell: {cell}");
        }
    }

    /// …while a currency code sitting where a real amount puts one is fine.
    #[test]
    fn parse_amount_cell_tolerates_an_adjacent_currency_code() {
        assert_eq!(parse_amount_cell("12,50 EUR"), Some(1_250));
        assert_eq!(parse_amount_cell("EUR 12,50"), Some(1_250));
        assert_eq!(parse_amount_cell("-60.80 usd"), Some(-6_080));
    }

    /// Currency-code stripping slices a string this crate did not create.
    /// Multi-byte characters sitting where an ASCII code would be must not
    /// slice mid-character — that's a panic, and it takes the import with it.
    #[test]
    fn parse_amount_cell_does_not_panic_on_multibyte_text() {
        for cell in ["ÉÛR 12", "€€€", "£€ 1,00", "ÀÉÎ", "日本 12"] {
            let _ = parse_amount_cell(cell);
        }
    }

    #[test]
    fn parse_amount_cell_handles_a_currency_symbol() {
        assert_eq!(parse_amount_cell("-€60.80"), Some(-6_080));
        assert_eq!(parse_amount_cell("£1,234.56"), Some(123_456));
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

    /// The "Pointage operation" bug, isolated: a constant column parses
    /// perfectly and must still score zero as an amount.
    #[test]
    fn amount_plausibility_rejects_a_constant_flag_column() {
        assert_eq!(amount_plausibility(&["0", "0", "0", "0"]), 0.0);
        assert_eq!(amount_plausibility(&["1", "1", "1"]), 0.0);
    }

    /// An all-zero column is a flag however few rows the file has.
    #[test]
    fn amount_plausibility_rejects_an_all_zero_column_at_any_size() {
        assert_eq!(amount_plausibility(&["0"]), 0.0);
        assert_eq!(amount_plausibility(&["0", "0"]), 0.0);
    }

    /// …but the credit half of a debit/credit pair legitimately holds a
    /// single value when the statement covers one income row, and must not
    /// be disqualified for being trivially "constant".
    #[test]
    fn amount_plausibility_accepts_a_column_holding_a_single_real_amount() {
        assert_eq!(amount_plausibility(&["+2521,14"]), 1.0);
    }

    #[test]
    fn amount_plausibility_prefers_values_carrying_cents_or_a_sign() {
        let money = amount_plausibility(&["-9,99", "+2521,14", "-3,00"]);
        let bare_integers = amount_plausibility(&["1", "2", "3"]);
        assert!(money > bare_integers);
        assert_eq!(money, 1.0);
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
    fn fold_strips_accents_and_case() {
        assert_eq!(fold("Débit"), "debit");
        assert_eq!(fold("Catégorie"), "categorie");
        assert_eq!(fold("PRÉLÈVEMENT"), "prelevement");
    }
}
