-- Column mappings the user corrected in the import dialog, so the same bank
-- export doesn't have to be re-mapped every month.
--
-- `signature` identifies a *file layout*, not a file: the folded header row
-- where there is one, and otherwise the delimiter plus a per-column shape
-- profile (see `file_signature` in crates/infra-csv). It carries a scheme
-- version prefix, so changing how signatures are built retires these rows
-- rather than matching them wrongly.
--
-- `mapping_json` is the serialized `ColumnMappingDto` owned by src-tauri.
-- It is stored opaque on purpose: which column means what is an adapter
-- concern with no domain invariants attached, so there is no aggregate here
-- to model and nothing in `domain` needs to know CSV files have columns.
-- A row is written only when an import is actually committed — a mapping the
-- user was still editing is not one they endorsed — and re-written on every
-- subsequent commit, so a bad remembered mapping heals itself the next time
-- the user corrects it.
CREATE TABLE csv_import_mappings (
  signature TEXT PRIMARY KEY,
  mapping_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
