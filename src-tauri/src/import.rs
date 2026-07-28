use chrono::NaiveDate;
use scrat_application::transaction_service::ImportRow;
use scrat_domain::account::AccountId;
use scrat_domain::category::CategoryId;
use scrat_infra_csv::build_preview;
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
    pub source: String,
    /// The CSV's own "Category"/"Catégorie" column, if it has one —
    /// informational only, shown alongside the row but not applied: every
    /// imported row still gets the one category chosen for the whole
    /// import.
    pub csv_category: Option<String>,
    /// Default checked/unchecked state — unparseable rows start unchecked.
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
                include_by_default: row.is_valid(),
                source: row.source,
                csv_category: row.csv_category,
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
    pub source: String,
    /// The CSV's own Category column text for this row, if any — when
    /// present, it's matched to an existing category by name (creating a
    /// new top-level one if nothing matches) instead of the row falling
    /// back to the category chosen for the whole import.
    pub category: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportSummaryDto {
    pub imported: usize,
    pub skipped_duplicates: usize,
}

struct ParsedCommitRow {
    date: NaiveDate,
    amount_minor_units: i64,
    source: String,
    category: Option<String>,
}

#[tauri::command]
pub fn commit_csv_import(
    state: State<DbState>,
    rows: Vec<ImportCommitRowDto>,
    category_id: Option<String>,
    account_id: String,
) -> Result<ImportSummaryDto, String> {
    let category_id = category_id
        .map(|id| CategoryId::parse(&id).map_err(|e| e.to_string()))
        .transpose()?;
    let account_id = AccountId::parse(&account_id).map_err(|e| e.to_string())?;
    let parsed_rows = rows
        .into_iter()
        .map(|row| {
            Ok(ParsedCommitRow {
                date: parse_date(&row.date)?,
                amount_minor_units: row.amount_minor_units,
                source: row.source,
                category: row.category,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    with_service(&state, |s| {
        let default_category_id = match category_id {
            Some(id) => id,
            None => s.get_or_create_default_category()?,
        };
        let import_rows = parsed_rows
            .into_iter()
            .map(|row| {
                let category_id = match row.category.as_deref().map(str::trim) {
                    Some(name) if !name.is_empty() => s.get_or_create_category_by_name(name)?,
                    _ => default_category_id,
                };
                Ok(ImportRow {
                    date: row.date,
                    amount_minor_units: row.amount_minor_units,
                    source: row.source,
                    category_id,
                })
            })
            .collect::<Result<Vec<_>, scrat_application::transaction_service::ApplicationError>>()?;
        s.import_transactions(&import_rows, account_id)
    })
    .map(|outcome| ImportSummaryDto {
        imported: outcome.imported,
        skipped_duplicates: outcome.skipped_duplicates,
    })
}
