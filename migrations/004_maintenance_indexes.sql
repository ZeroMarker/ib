-- Indexes backing the expired-row sweep.
--
-- SESSIONS and EMAIL_VERIFICATIONS only ever grow if expired rows are kept
-- around: reads filter on EXPIRES_AT but nothing used to delete anything.
-- `ib` prunes both on every login (see auth::purge_expired), and without
-- these indexes that prune degrades to a full table scan.

CREATE INDEX IF NOT EXISTS IX_SESSIONS_EXPIRES_AT ON SESSIONS (EXPIRES_AT);

CREATE INDEX IF NOT EXISTS IX_EMAIL_VERIFICATIONS_EXPIRES_AT
    ON EMAIL_VERIFICATIONS (EXPIRES_AT);

-- The overview endpoint is the hot read: it lists one account's fills newest
-- first. ORDERS/FILLS/POSITIONS are only ever queried per account.
CREATE INDEX IF NOT EXISTS IX_ORDERS_ACCOUNT_STATUS ON ORDERS (ACCOUNT_ID, STATUS);

CREATE INDEX IF NOT EXISTS IX_FILLS_ACCOUNT_TIME ON FILLS (ACCOUNT_ID, EXEC_TIME DESC);
