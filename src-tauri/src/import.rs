use chrono::NaiveDate;
use scrat_application::transaction_service::ImportRow;
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_domain::ports::TransferRuleRepository;
use scrat_domain::transaction::OperationKind;
use scrat_infra_csv::{
    AmountSource, ColumnMapping, DATE_FORMATS, apply_mapping, detect_mapping, file_signature,
    parse_file,
};
use scrat_infra_sqlite::{SqliteTransferRuleRepository, get_csv_mapping, save_csv_mapping};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbState;
use crate::errors::{AppError, codes};
use crate::transactions::{parse_date, with_service};

#[derive(Debug, Serialize)]
pub struct ImportPreviewRowDto {
    /// `None` when the row's date cell didn't parse — the frontend should
    /// disable inclusion for such a row rather than let the user check it.
    pub date: Option<String>,
    pub amount_minor_units: Option<i64>,
    pub description: String,
    /// The CSV's own "Category"/"Catégorie" column, if it has one — applied
    /// on import (creating the category if needed) instead of the row
    /// falling back to the category chosen for the whole import.
    pub csv_category: Option<String>,
    /// The CSV's own "Subcategory"/"Sous-catégorie" column, if it has one —
    /// nests under `csv_category` on import, mirroring the app's own export
    /// format.
    pub csv_subcategory: Option<String>,
    /// How the money moved, as read from the file's own operation-type
    /// column or — failing that — from the description text. Shown in the
    /// preview so the user can see what will be stored before committing.
    pub operation_kind: String,
    /// True when the row looks like a bank's opening/closing balance line
    /// rather than a real transaction — surfaced so the frontend can flag
    /// it to the user, in addition to defaulting it unchecked.
    pub is_likely_balance_row: bool,
    /// Default checked/unchecked state — unparseable rows and rows that
    /// look like an opening/closing balance line start unchecked.
    pub include_by_default: bool,
    pub raw: Vec<String>,
}

/// One column of the file as the import dialog offers it for re-pointing:
/// what the file calls it, plus real values so a headerless column is still
/// recognizable.
#[derive(Debug, Serialize)]
pub struct ColumnSummaryDto {
    pub index: usize,
    pub header: Option<String>,
    pub samples: Vec<String>,
}

/// Where amounts come from. Serialized as a tagged union so the frontend can
/// switch the editor between one signed column and a debit/credit pair
/// without a second nullable field to keep consistent.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AmountSourceDto {
    Single { column: usize },
    DebitCredit { debit: usize, credit: usize },
}

/// The detector's guess, in the exact shape the user edits and sends back.
#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnMappingDto {
    pub has_header: bool,
    pub column_count: usize,
    pub date_column: Option<usize>,
    pub date_format: String,
    pub amount: Option<AmountSourceDto>,
    pub description_columns: Vec<usize>,
    pub category_column: Option<usize>,
    pub subcategory_column: Option<usize>,
    pub currency_column: Option<usize>,
    pub account_column: Option<usize>,
    pub operation_kind_column: Option<usize>,
}

impl ColumnMappingDto {
    fn from_domain(mapping: &ColumnMapping) -> Self {
        Self {
            has_header: mapping.has_header,
            column_count: mapping.column_count,
            date_column: mapping.date_column,
            date_format: mapping.date_format.clone(),
            amount: mapping.amount.map(|a| match a {
                AmountSource::Single(column) => AmountSourceDto::Single { column },
                AmountSource::DebitCredit { debit, credit } => {
                    AmountSourceDto::DebitCredit { debit, credit }
                }
            }),
            description_columns: mapping.description_columns.clone(),
            category_column: mapping.category_column,
            subcategory_column: mapping.subcategory_column,
            currency_column: mapping.currency_column,
            account_column: mapping.account_column,
            operation_kind_column: mapping.operation_kind_column,
        }
    }

    /// `delimiter` isn't part of the DTO: it's sniffed from the file every
    /// time and there is nothing for the user to correct about it, so it's
    /// carried over from the freshly parsed file rather than round-tripped.
    fn into_domain(self, delimiter: char) -> ColumnMapping {
        ColumnMapping {
            delimiter,
            has_header: self.has_header,
            column_count: self.column_count,
            date_column: self.date_column,
            // A format the frontend didn't get from `DATE_FORMATS` would be
            // handed straight to chrono as a parse pattern. Reject anything
            // off the list rather than let an arbitrary string through.
            date_format: DATE_FORMATS
                .iter()
                .find(|(fmt, _)| *fmt == self.date_format)
                .map_or_else(|| DATE_FORMATS[1].0.to_string(), |(fmt, _)| fmt.to_string()),
            amount: self.amount.map(|a| match a {
                AmountSourceDto::Single { column } => AmountSource::Single(column),
                AmountSourceDto::DebitCredit { debit, credit } => {
                    AmountSource::DebitCredit { debit, credit }
                }
            }),
            description_columns: self.description_columns,
            category_column: self.category_column,
            subcategory_column: self.subcategory_column,
            currency_column: self.currency_column,
            account_column: self.account_column,
            operation_kind_column: self.operation_kind_column,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DateFormatOptionDto {
    pub pattern: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct ImportPreviewDto {
    pub rows: Vec<ImportPreviewRowDto>,
    /// How the rows above were read. Sent back on every preview so the
    /// dialog always shows the mapping actually in force, whether it was
    /// detected or supplied by the user.
    pub mapping: ColumnMappingDto,
    pub columns: Vec<ColumnSummaryDto>,
    pub date_formats: Vec<DateFormatOptionDto>,
    /// Identifies this file's *layout*, so the mapping can be remembered
    /// against it once an import is committed. Sent back on commit.
    pub signature: String,
    /// True when the mapping above came from a previously committed import
    /// of the same layout rather than from detection — surfaced so the user
    /// knows why the columns are already right, and that changing them
    /// changes what's remembered.
    pub remembered: bool,
    /// Fraction of rows that yielded a usable date / non-zero amount —
    /// measured from the parsed result, so a mapping that produces nothing
    /// reports 0, not 100.
    pub date_confidence: f64,
    pub amount_confidence: f64,
}

/// Above this, a file is almost certainly not a bank's CSV export — a
/// genuine one is a few thousand rows at most. Rejecting early avoids
/// shipping an enormous byte array over the Tauri IPC bridge and parsing it
/// just to find that out.
const MAX_CSV_FILE_BYTES: usize = 20 * 1024 * 1024;

/// Previews `bytes`. With an explicit `mapping` it is applied verbatim — the
/// path the dialog takes after the user corrects a column. Without one, a
/// mapping remembered from a previous import of the same layout wins over
/// detection, and detection is the fallback.
#[tauri::command]
pub fn preview_csv_import(
    state: State<DbState>,
    bytes: Vec<u8>,
    mapping: Option<ColumnMappingDto>,
) -> Result<ImportPreviewDto, AppError> {
    if bytes.len() > MAX_CSV_FILE_BYTES {
        return Err(AppError::new(codes::CSV_FILE_TOO_LARGE)
            .with(
                "size_mb",
                format!("{:.1}", bytes.len() as f64 / (1024.0 * 1024.0)),
            )
            .with("limit_mb", MAX_CSV_FILE_BYTES / (1024 * 1024)));
    }
    let mut file = parse_file(&bytes);
    let signature = file_signature(&file);

    // A remembered mapping is only worth anything if the file still has the
    // same number of columns; a bank that adds one has invalidated it.
    let remembered_mapping = mapping.is_none().then(|| {
        let guard = state.0.lock().ok()?;
        let conn = guard.as_ref()?;
        let json = get_csv_mapping(conn, &signature).ok()??;
        let dto: ColumnMappingDto = serde_json::from_str(&json).ok()?;
        (dto.column_count == file.column_count).then_some(dto)
    });
    let remembered = matches!(remembered_mapping, Some(Some(_)));

    let mapping = match mapping.or(remembered_mapping.flatten()) {
        Some(dto) => {
            let mapping = dto.into_domain(file.delimiter);
            file.set_has_header(mapping.has_header);
            mapping
        }
        None => detect_mapping(&file),
    };
    let preview = apply_mapping(&file, &mapping);

    Ok(ImportPreviewDto {
        rows: preview
            .rows
            .into_iter()
            .map(|row| ImportPreviewRowDto {
                date: row.date.map(|d| d.format("%Y-%m-%d").to_string()),
                amount_minor_units: row.amount_minor_units,
                include_by_default: row.is_valid() && !row.is_likely_balance_row,
                operation_kind: row.operation_kind.as_str().to_string(),
                description: row.description,
                csv_category: row.csv_category,
                csv_subcategory: row.csv_subcategory,
                is_likely_balance_row: row.is_likely_balance_row,
                raw: row.raw,
            })
            .collect(),
        mapping: ColumnMappingDto::from_domain(&preview.mapping),
        columns: preview
            .columns
            .into_iter()
            .map(|c| ColumnSummaryDto {
                index: c.index,
                header: c.header,
                samples: c.samples,
            })
            .collect(),
        date_formats: DATE_FORMATS
            .iter()
            .map(|(pattern, label)| DateFormatOptionDto {
                pattern: pattern.to_string(),
                label: label.to_string(),
            })
            .collect(),
        signature,
        remembered,
        date_confidence: preview.date_confidence,
        amount_confidence: preview.amount_confidence,
    })
}

#[derive(Debug, Deserialize)]
pub struct DuplicateCheckRowDto {
    pub date: String,
    pub amount_minor_units: i64,
    pub description: String,
}

/// Flags which of `rows` already sit in `account_id`'s ledger under the same
/// date, amount, and description, so the import dialog can default those
/// rows unticked. Checked against whichever account the import is currently
/// targeting — `account_id` absent falls back to the app default, same as
/// `commit_csv_import` — and re-run whenever that target changes, since a
/// duplicate is only a duplicate on the account it collides with.
///
/// This is a hint, not a constraint: nothing here stops a flagged row from
/// being imported anyway, and the ledger itself enforces no such uniqueness
/// (see [`scrat_domain::transaction::TransactionFingerprint`]).
#[tauri::command]
pub fn check_duplicate_transactions(
    state: State<DbState>,
    account_id: Option<String>,
    rows: Vec<DuplicateCheckRowDto>,
) -> Result<Vec<bool>, AppError> {
    let account_id = account_id.map(|id| AccountId::parse(&id)).transpose()?;
    let parsed_rows = rows
        .into_iter()
        .map(|r| Ok((parse_date(&r.date)?, r.amount_minor_units, r.description)))
        .collect::<Result<Vec<_>, AppError>>()?;

    let account_id = match account_id {
        Some(id) => id,
        None => {
            let guard = state.0.lock().unwrap();
            let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
            match crate::accounts::resolve_default_account_id(conn)? {
                Some(id) => id,
                // No destination account resolved yet — nothing to compare
                // against, so nothing is a duplicate.
                None => return Ok(vec![false; parsed_rows.len()]),
            }
        }
    };

    with_service(&state, |s| s.find_duplicate_rows(account_id, &parsed_rows))
}

#[derive(Debug, Deserialize)]
pub struct ImportCommitRowDto {
    pub date: String,
    pub amount_minor_units: i64,
    pub description: String,
    /// The CSV's own Category column text for this row, if any — when
    /// present, it's matched to an existing category by name (creating a
    /// new top-level one if nothing matches). When absent, the row is
    /// categorized from the most recent past transaction with the same
    /// description text, if one exists, before falling back to the category
    /// chosen for the whole import.
    pub category: Option<String>,
    /// The CSV's own Subcategory column text for this row, if any — nests
    /// under `category` (found or created) instead of being applied on its
    /// own. When `category` is given but this is blank, the most recent past
    /// transaction with the same description text that was filed under `category`
    /// (or one of its subcategories) supplies the subcategory instead,
    /// before falling back to the bare top-level `category`.
    pub subcategory: Option<String>,
    /// The stored `OperationKind` string the preview reported for this row.
    /// Absent or unrecognized falls back to `card`, which is the same rule
    /// the detector itself applies to a file that never says how the money
    /// moved — a malformed value from the frontend shouldn't fail an import
    /// over a purely descriptive field.
    pub operation_kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportSummaryDto {
    pub imported: usize,
    /// How many imported rows a transfer rule recognized as money moving to
    /// another of the user's own accounts, and so also wrote a mirrored leg
    /// on that account. Surfaced so the import confirmation can say the
    /// counterpart was updated too — otherwise those entries appear on an
    /// account the user never imported anything into.
    pub mirrored: usize,
}

struct ParsedCommitRow {
    date: NaiveDate,
    amount_minor_units: i64,
    description: String,
    category: Option<String>,
    subcategory: Option<String>,
    operation_kind: OperationKind,
}

/// Remembers the mapping this import was committed with, against the file
/// layout it was read from.
///
/// Only on commit: a mapping the user was still editing in the dialog is not
/// one they endorsed. Failing to remember is not worth failing an import
/// that already succeeded, so an error here is swallowed — the user simply
/// re-corrects the columns next time, which is the behavior they had before
/// this existed.
fn remember_mapping(
    conn: &scrat_infra_sqlite::Connection,
    signature: Option<&str>,
    mapping: Option<&ColumnMappingDto>,
) {
    let (Some(signature), Some(mapping)) = (signature, mapping) else {
        return;
    };
    if signature.is_empty() {
        return;
    }
    if let Ok(json) = serde_json::to_string(mapping) {
        let _ = save_csv_mapping(conn, signature, &json);
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn commit_csv_import(
    state: State<DbState>,
    rows: Vec<ImportCommitRowDto>,
    category_id: Option<String>,
    account_id: Option<String>,
    signature: Option<String>,
    mapping: Option<ColumnMappingDto>,
    // When true, reverses the usual precedence: a row is first categorized
    // from the most recent past transaction with the same description text
    // (as long as that past transaction is itself categorized, not just
    // sitting in "Uncategorized"), and only falls back to the CSV's own
    // category — or the default — when no such history exists. Off by
    // default so existing imports keep trusting the file's own column.
    prioritize_historical_category: bool,
    // Whether past transactions are consulted at all to categorize a row
    // that the CSV itself leaves uncategorized (or, under
    // `prioritize_historical_category`, to override the CSV's own column).
    // On by default; turning it off makes every row fall back to the CSV's
    // category or the chosen default, ignoring history entirely.
    detect_category_from_history: bool,
) -> Result<ImportSummaryDto, AppError> {
    let category_id = category_id.map(|id| CategoryId::parse(&id)).transpose()?;
    let account_id = account_id.map(|id| AccountId::parse(&id)).transpose()?;
    let parsed_rows = rows
        .into_iter()
        .map(|row| {
            Ok(ParsedCommitRow {
                date: parse_date(&row.date)?,
                amount_minor_units: row.amount_minor_units,
                description: row.description,
                category: row.category,
                subcategory: row.subcategory,
                operation_kind: row
                    .operation_kind
                    .as_deref()
                    .and_then(|raw| OperationKind::parse(raw).ok())
                    .unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let (default_category_id, account_id, transfer_rules) = {
        let guard = state.0.lock().unwrap();
        let conn = guard.as_ref().ok_or_else(AppError::db_locked)?;
        let default_category_id = match category_id {
            Some(id) => id,
            None => crate::categories::resolve_default_category_id(conn)?,
        };
        let account_id = match account_id {
            Some(id) => id,
            None => crate::accounts::resolve_default_account_id(conn)?
                .ok_or_else(|| AppError::new(codes::NO_DESTINATION_ACCOUNT))?,
        };
        // Read here rather than inside `with_service` below, which locks the
        // same mutex and would deadlock.
        let transfer_rules = SqliteTransferRuleRepository::new(conn).list_all()?;
        (default_category_id, account_id, transfer_rules)
    };

    with_service(&state, |s| {
        let import_rows = parsed_rows
            .into_iter()
            .map(|row| {
                let subcategory = row
                    .subcategory
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let csv_category = row
                    .category
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let category_id = if prioritize_historical_category {
                    // A match that resolved to the default category isn't a
                    // real historical categorization — it's just what every
                    // uncategorized transaction falls back to — so it
                    // shouldn't out-rank a category the CSV actually names.
                    let historical = if detect_category_from_history {
                        s.find_category_for_description(&row.description)?
                            .filter(|id| *id != default_category_id)
                    } else {
                        None
                    };
                    match historical {
                        Some(historical_id) => historical_id,
                        None => match csv_category {
                            Some(name) => s.get_or_create_category_path(name, subcategory)?,
                            None => default_category_id,
                        },
                    }
                } else {
                    match csv_category {
                        Some(name) => match subcategory {
                            Some(sub) => s.get_or_create_category_path(name, Some(sub))?,
                            None => {
                                let historical = if detect_category_from_history {
                                    s.find_category_for_description_in_category(
                                        &row.description,
                                        name,
                                    )?
                                } else {
                                    None
                                };
                                match historical {
                                    Some(historical_id) => historical_id,
                                    None => s.get_or_create_category_path(name, None)?,
                                }
                            }
                        },
                        None => {
                            let historical = if detect_category_from_history {
                                s.find_category_for_description(&row.description)?
                            } else {
                                None
                            };
                            historical.unwrap_or(default_category_id)
                        }
                    }
                };
                Ok(ImportRow {
                    date: row.date,
                    amount_minor_units: row.amount_minor_units,
                    description: row.description,
                    category_id,
                    operation_kind: row.operation_kind,
                })
            })
            .collect::<Result<Vec<_>, scrat_application::transaction_service::ApplicationError>>(
            )?;
        s.import_transactions(&import_rows, account_id, &transfer_rules)
    })
    .map(|outcome| {
        // After `with_service` has released the mutex — it locks the same
        // one, and re-entering it here would deadlock.
        if let Ok(guard) = state.0.lock()
            && let Some(conn) = guard.as_ref()
        {
            remember_mapping(conn, signature.as_deref(), mapping.as_ref());
        }
        ImportSummaryDto {
            imported: outcome.imported,
            mirrored: outcome.mirrored,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> ColumnMapping {
        ColumnMapping {
            delimiter: ';',
            has_header: true,
            column_count: 8,
            date_column: Some(0),
            date_format: "%d/%m/%Y".to_string(),
            amount: Some(AmountSource::Single(1)),
            description_columns: vec![3, 4],
            category_column: Some(5),
            subcategory_column: Some(6),
            currency_column: Some(2),
            account_column: Some(7),
            operation_kind_column: None,
        }
    }

    /// Everything the user can correct in the dialog has to survive the trip
    /// out to the frontend and back, or a mapping would quietly degrade each
    /// time the dialog re-rendered it.
    #[test]
    fn a_mapping_survives_a_round_trip_through_the_dto() {
        let original = mapping();

        let restored = ColumnMappingDto::from_domain(&original).into_domain(original.delimiter);

        assert_eq!(restored, original);
    }

    #[test]
    fn a_debit_credit_pair_survives_a_round_trip() {
        let original = ColumnMapping {
            amount: Some(AmountSource::DebitCredit {
                debit: 2,
                credit: 3,
            }),
            ..mapping()
        };

        let restored = ColumnMappingDto::from_domain(&original).into_domain(original.delimiter);

        assert_eq!(restored.amount, original.amount);
    }

    /// A file whose amount column detection couldn't place round-trips as
    /// "still unplaced" rather than collapsing to column 0 — which would
    /// silently import every row with whatever the first column happens to
    /// hold.
    #[test]
    fn an_undetected_amount_column_stays_undetected() {
        let original = ColumnMapping {
            amount: None,
            date_column: None,
            ..mapping()
        };

        let restored = ColumnMappingDto::from_domain(&original).into_domain(original.delimiter);

        assert_eq!(restored.amount, None);
        assert_eq!(restored.date_column, None);
    }

    /// The delimiter is deliberately absent from the DTO — it's re-sniffed
    /// from the file on every preview. `into_domain` must take the caller's,
    /// not resurrect a stale one.
    #[test]
    fn the_delimiter_comes_from_the_caller_not_the_dto() {
        let original = mapping();

        let restored = ColumnMappingDto::from_domain(&original).into_domain(',');

        assert_eq!(restored.delimiter, ',');
    }

    #[test]
    fn every_offered_date_format_survives_a_round_trip() {
        for (pattern, _label) in DATE_FORMATS {
            let original = ColumnMapping {
                date_format: (*pattern).to_string(),
                ..mapping()
            };

            let restored = ColumnMappingDto::from_domain(&original).into_domain(';');

            assert_eq!(
                restored.date_format, *pattern,
                "{pattern} should not be rewritten"
            );
        }
    }

    /// The date format is handed to chrono as a parse pattern, so it must
    /// come from the fixed list the dialog offers and nowhere else. A value
    /// the frontend didn't get from `DATE_FORMATS` — whether a typo, a stale
    /// remembered mapping, or a hand-crafted IPC payload — falls back to the
    /// day-first default instead of reaching chrono verbatim.
    #[test]
    fn a_date_format_off_the_offered_list_falls_back_to_the_default() {
        let unoffered = [
            "%Y",
            "%d/%m/%y",
            "not a format at all",
            "",
            "%d/%m/%Y ",
            "%C%C%C%C%C%C%C%C",
        ];

        for pattern in unoffered {
            let original = ColumnMapping {
                date_format: pattern.to_string(),
                ..mapping()
            };

            let restored = ColumnMappingDto::from_domain(&original).into_domain(';');

            assert_eq!(
                restored.date_format, DATE_FORMATS[1].0,
                "{pattern:?} should have fallen back to the default"
            );
        }
    }

    /// The fallback is day-first, matching the European bank exports this app
    /// is built around — reading `03/07` as 3 July, not 7 March.
    #[test]
    fn the_date_format_fallback_is_day_first() {
        assert_eq!(DATE_FORMATS[1].0, "%d/%m/%Y");
    }
}
