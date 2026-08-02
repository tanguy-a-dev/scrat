-- "source" meant four unrelated things across this codebase: a transaction's
-- raw description text, the text an account/transfer pattern is matched
-- against, the originating account of a transfer, and a file path being
-- restored from. The first and third collided inside 0005_transfers.sql
-- itself, seven lines apart. Renaming the text to `description` frees
-- "source account" to mean only the account a transfer leaves from.
--
-- `dedup_key` is renamed in the same pass: 0004 deliberately dropped its
-- UNIQUE constraint, so nothing is deduplicated on it and the name promised
-- behaviour the app does not have. `fingerprint` names what it is — a stable
-- hash of (account, date, amount, normalized description) kept as a candidate
-- key for a future "find likely duplicates" review feature.
--
-- Pure renames: no data is rewritten, so an existing ledger survives
-- untouched.

ALTER TABLE transactions RENAME COLUMN source TO description;
ALTER TABLE transactions RENAME COLUMN dedup_key TO fingerprint;

ALTER TABLE account_source_patterns RENAME TO account_description_patterns;
