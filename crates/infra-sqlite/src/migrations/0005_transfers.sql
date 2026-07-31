-- Transfers between two of the user's own accounts, and the balance
-- adjustments used to reconcile an account whose statements can't be
-- exported (a neobank, say).
--
-- Both are ordinary rows in `transactions` so account balance stays
-- `opening_balance + SUM(transactions)` with no second code path — `role`
-- exists only so reporting can leave them out of income/expense totals.
-- Moving money between your own accounts isn't spending, and a
-- reconciliation delta is a correction, not earnings.

ALTER TABLE transactions ADD COLUMN role TEXT NOT NULL DEFAULT 'normal';

-- Shared by the two legs of one transfer: the outflow on the source account
-- and the mirrored inflow on the counterpart. Null for every other role.
ALTER TABLE transactions ADD COLUMN transfer_group_id TEXT;

CREATE INDEX idx_transactions_transfer_group ON transactions(transfer_group_id);

-- Matched (as a normalized lowercase substring, same as
-- account_source_patterns) against an imported row's source text to
-- recognize it as a transfer to `counterpart_account_id`. A pattern maps to
-- exactly one counterpart — two rules disagreeing about where the same
-- source text sends money has no sensible resolution, so it's rejected at
-- write time.
CREATE TABLE transfer_rules (
  id TEXT PRIMARY KEY,
  pattern TEXT NOT NULL UNIQUE,
  counterpart_account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_transfer_rules_counterpart ON transfer_rules(counterpart_account_id);
