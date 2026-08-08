-- Display order for accounts, set by dragging in the Accounts list and
-- reused as-is by the Overview grid. Purely a persistence-layer ordering
-- concern (like created_at/updated_at) — it has no domain meaning, so it
-- never enters the Account aggregate itself.
--
-- Existing rows backfill in their current (alphabetical) order, so an
-- upgrade doesn't visibly reshuffle anyone's accounts.

ALTER TABLE accounts ADD COLUMN position INTEGER NOT NULL DEFAULT 0;

UPDATE accounts SET position = (
    SELECT COUNT(*) FROM accounts a2
    WHERE a2.name < accounts.name
       OR (a2.name = accounts.name AND a2.id < accounts.id)
);
