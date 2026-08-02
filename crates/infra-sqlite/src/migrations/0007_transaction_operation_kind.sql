-- How the money moved: the payment instrument the bank printed on the
-- statement ("Carte bancaire", "Virement", "Frais bancaires"…), normalized
-- to the closed set in `OperationKind`.
--
-- A third axis, independent of the two already stored. `role` says what a
-- movement means to the ledger and is what reporting keys off; this says
-- only which instrument carried it, and never changes whether a row counts
-- as spending. Note in particular that `operation_kind = 'bank_transfer'`
-- is NOT `role = 'transfer'`: rent paid by wire is ordinary spending that
-- happened to be paid by wire.
--
-- Existing rows backfill to 'card'. That's the same rule the CSV importer
-- applies to a file with no "Type opération" column — card is the commonest
-- instrument, and it's the likeliest reading of a row that never recorded
-- one. It is a default, not a claim the bank made: a re-import of the same
-- statement with its type column present will label the new rows properly.

ALTER TABLE transactions ADD COLUMN operation_kind TEXT NOT NULL DEFAULT 'card';
