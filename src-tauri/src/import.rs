use chrono::NaiveDate;
use scrat_application::transaction_service::ImportRow;
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_domain::ports::TransferRuleRepository;
use scrat_infra_csv::build_preview;
use scrat_infra_sqlite::SqliteTransferRuleRepository;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbState;
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
    /// True when the row looks like a bank's opening/closing balance line
    /// rather than a real transaction — surfaced so the frontend can flag
    /// it to the user, in addition to defaulting it unchecked.
    pub is_likely_balance_row: bool,
    /// Default checked/unchecked state — unparseable rows and rows that
    /// look like an opening/closing balance line start unchecked.
    pub include_by_default: bool,
    pub raw: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportPreviewDto {
    pub rows: Vec<ImportPreviewRowDto>,
    pub date_confidence: f64,
    pub amount_confidence: f64,
}

#[tauri::command]
pub fn preview_csv_import(bytes: Vec<u8>) -> ImportPreviewDto {
    let preview = build_preview(&bytes);
    ImportPreviewDto {
        rows: preview
            .rows
            .into_iter()
            .map(|row| ImportPreviewRowDto {
                date: row.date.map(|d| d.format("%Y-%m-%d").to_string()),
                amount_minor_units: row.amount_minor_units,
                include_by_default: row.is_valid() && !row.is_likely_balance_row,
                description: row.description,
                csv_category: row.csv_category,
                csv_subcategory: row.csv_subcategory,
                is_likely_balance_row: row.is_likely_balance_row,
                raw: row.raw,
            })
            .collect(),
        date_confidence: preview.date_confidence,
        amount_confidence: preview.amount_confidence,
    }
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
}

#[tauri::command]
pub fn commit_csv_import(
    state: State<DbState>,
    rows: Vec<ImportCommitRowDto>,
    category_id: Option<String>,
    account_id: Option<String>,
) -> Result<ImportSummaryDto, String> {
    let category_id = category_id
        .map(|id| CategoryId::parse(&id).map_err(|e| e.to_string()))
        .transpose()?;
    let account_id = account_id
        .map(|id| AccountId::parse(&id).map_err(|e| e.to_string()))
        .transpose()?;
    let parsed_rows = rows
        .into_iter()
        .map(|row| {
            Ok(ParsedCommitRow {
                date: parse_date(&row.date)?,
                amount_minor_units: row.amount_minor_units,
                description: row.description,
                category: row.category,
                subcategory: row.subcategory,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let (default_category_id, account_id, transfer_rules) = {
        let guard = state.0.lock().unwrap();
        let conn = guard
            .as_ref()
            .ok_or_else(|| "database is locked".to_string())?;
        let default_category_id = match category_id {
            Some(id) => id,
            None => crate::categories::resolve_default_category_id(conn)?,
        };
        let account_id = match account_id {
            Some(id) => id,
            None => crate::accounts::resolve_default_account_id(conn)?.ok_or_else(|| {
                "no destination account chosen, and no default account is set — pick one, or set a default in Accounts".to_string()
            })?,
        };
        // Read here rather than inside `with_service` below, which locks the
        // same mutex and would deadlock.
        let transfer_rules = SqliteTransferRuleRepository::new(conn)
            .list_all()
            .map_err(|e| e.to_string())?;
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
                let category_id = match row
                    .category
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    Some(name) => match subcategory {
                        Some(sub) => s.get_or_create_category_path(name, Some(sub))?,
                        None => match s
                            .find_category_for_description_in_category(&row.description, name)?
                        {
                            Some(historical_id) => historical_id,
                            None => s.get_or_create_category_path(name, None)?,
                        },
                    },
                    None => s
                        .find_category_for_description(&row.description)?
                        .unwrap_or(default_category_id),
                };
                Ok(ImportRow {
                    date: row.date,
                    amount_minor_units: row.amount_minor_units,
                    description: row.description,
                    category_id,
                })
            })
            .collect::<Result<Vec<_>, scrat_application::transaction_service::ApplicationError>>(
            )?;
        s.import_transactions(&import_rows, account_id, &transfer_rules)
    })
    .map(|outcome| ImportSummaryDto {
        imported: outcome.imported,
        mirrored: outcome.mirrored,
    })
}
