-- Whether an account's starting point has actually been established.
--
-- `opening_balance_minor_units` alone can't say. It defaults to 0, and 0 is
-- also a legitimate answer ("this account really did begin empty"), so the
-- two states were indistinguishable — which meant the app could not tell a
-- balance it knows from one it is guessing at, and could not warn about the
-- second without also nagging the first forever.
--
-- Existing rows backfill as established only where a non-zero opening
-- balance was recorded. That's the conservative reading: a non-zero value is
-- something the user typed on purpose, while a 0 is far more likely to be
-- the default nobody ever filled in — and those are exactly the accounts
-- whose imported history is silently off by a constant, which is what the
-- new prompt exists to catch. An account that truly started at zero is
-- mislabelled as unset once, and one answer from the user settles it
-- permanently.

ALTER TABLE accounts ADD COLUMN opening_balance_set INTEGER NOT NULL DEFAULT 0;
UPDATE accounts SET opening_balance_set = 1 WHERE opening_balance_minor_units != 0;
