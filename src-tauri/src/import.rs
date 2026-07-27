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
}

#[derive(Debug, Serialize)]
pub struct ImportSummaryDto {
    pub imported: usize,
    pub skipped_duplicates: usize,
}

#[tauri::command]
pub fn commit_csv_import(
    state: State<DbState>,
    rows: Vec<ImportCommitRowDto>,
    category_id: String,
    account_id: String,
) -> Result<ImportSummaryDto, String> {
    let category_id = CategoryId::parse(&category_id).map_err(|e| e.to_string())?;
    let account_id = AccountId::parse(&account_id).map_err(|e| e.to_string())?;
    let import_rows = rows
        .into_iter()
        .map(|row| {
            Ok(ImportRow {
                date: parse_date(&row.date)?,
                amount_minor_units: row.amount_minor_units,
                source: row.source,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    with_service(&state, |s| {
        s.import_transactions(&import_rows, category_id, account_id)
    })
    .map(|outcome| ImportSummaryDto {
        imported: outcome.imported,
        skipped_duplicates: outcome.skipped_duplicates,
    })
}
