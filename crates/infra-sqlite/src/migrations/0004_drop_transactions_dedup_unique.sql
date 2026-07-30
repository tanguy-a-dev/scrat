-- SQLite can't ALTER a column's UNIQUE constraint away, so recreate the
-- table without it: identical (account, date, amount, source) transactions
-- are legitimate (e.g. two identical coffees on the same day), not an error.
CREATE TABLE transactions_new (
  id TEXT PRIMARY KEY,
  date TEXT NOT NULL,
  amount_minor_units INTEGER NOT NULL,
  source TEXT NOT NULL,
  category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE RESTRICT,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
  dedup_key TEXT NOT NULL,
  created_at TEXT NOT NULL
);

INSERT INTO transactions_new
  SELECT id, date, amount_minor_units, source, category_id, account_id, dedup_key, created_at
  FROM transactions;

DROP TABLE transactions;
ALTER TABLE transactions_new RENAME TO transactions;

CREATE INDEX idx_transactions_date ON transactions(date);
CREATE INDEX idx_transactions_account ON transactions(account_id);
CREATE INDEX idx_transactions_category ON transactions(category_id);
