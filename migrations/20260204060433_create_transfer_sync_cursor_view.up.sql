-- Create materialized view for transfer sync cursor
-- Cursor algorithm:
-- 1. If there are unconfirmed transfers within the last 1 day, use the earliest unconfirmed transfer
-- 2. Otherwise, use the latest transfer
-- 3. If no transfers exist, return empty result
--
-- Note: ERC20 uses block_number for cursoring (EtherScan filters by block),
--       TRC20 uses block_timestamp for cursoring (TronScan filters by timestamp)

CREATE MATERIALIZED VIEW erc20_sync_cursor AS
WITH one_day_ago AS (
    SELECT EXTRACT(EPOCH FROM (NOW() - INTERVAL '1 day'))::BIGINT AS ts
)
SELECT
    e.chain,
    e.token_name,
    CASE
        -- If there are unconfirmed transfers within the last 1 day, use the earliest block_number
        WHEN EXISTS (
            SELECT 1 FROM erc20_token_transfers e2, one_day_ago
            WHERE e2.chain = e.chain
              AND e2.token_name = e.token_name
              AND e2.blockchain_confirmed = FALSE
              AND e2.block_timestamp >= one_day_ago.ts
        ) THEN (
            SELECT MIN(e3.block_number)
            FROM erc20_token_transfers e3, one_day_ago
            WHERE e3.chain = e.chain
              AND e3.token_name = e.token_name
              AND e3.blockchain_confirmed = FALSE
              AND e3.block_timestamp >= one_day_ago.ts
        )
        -- Otherwise, use the latest block_number
        ELSE MAX(e.block_number)
    END AS cursor_block_number,
    CASE
        WHEN EXISTS (
            SELECT 1 FROM erc20_token_transfers e2, one_day_ago
            WHERE e2.chain = e.chain
              AND e2.token_name = e.token_name
              AND e2.blockchain_confirmed = FALSE
              AND e2.block_timestamp >= one_day_ago.ts
        ) THEN TRUE
        ELSE FALSE
    END AS has_pending_confirmation
FROM erc20_token_transfers e
GROUP BY e.chain, e.token_name;

-- Create unique index for concurrent refresh
CREATE UNIQUE INDEX idx_erc20_sync_cursor_chain_token
ON erc20_sync_cursor (chain, token_name);

CREATE MATERIALIZED VIEW trc20_sync_cursor AS
WITH one_day_ago AS (
    SELECT EXTRACT(EPOCH FROM (NOW() - INTERVAL '1 day'))::BIGINT AS ts
)
SELECT
    t.token_name,
    CASE
        -- If there are unconfirmed transfers within the last 1 day, use the earliest timestamp
        WHEN EXISTS (
            SELECT 1 FROM trc20_token_transfers t2, one_day_ago
            WHERE t2.token_name = t.token_name
              AND t2.blockchain_confirmed = FALSE
              AND t2.block_timestamp >= one_day_ago.ts
        ) THEN (
            SELECT MIN(t3.block_timestamp)
            FROM trc20_token_transfers t3, one_day_ago
            WHERE t3.token_name = t.token_name
              AND t3.blockchain_confirmed = FALSE
              AND t3.block_timestamp >= one_day_ago.ts
        )
        -- Otherwise, use the latest timestamp
        ELSE MAX(t.block_timestamp)
    END AS cursor_block_timestamp,
    CASE
        WHEN EXISTS (
            SELECT 1 FROM trc20_token_transfers t2, one_day_ago
            WHERE t2.token_name = t.token_name
              AND t2.blockchain_confirmed = FALSE
              AND t2.block_timestamp >= one_day_ago.ts
        ) THEN TRUE
        ELSE FALSE
    END AS has_pending_confirmation
FROM trc20_token_transfers t
GROUP BY t.token_name;

-- Create unique index for concurrent refresh
CREATE UNIQUE INDEX idx_trc20_sync_cursor_token
ON trc20_sync_cursor (token_name);

-- Function to refresh the ERC20 materialized view
CREATE OR REPLACE FUNCTION refresh_erc20_sync_cursor()
RETURNS TRIGGER AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY erc20_sync_cursor;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Function to refresh the TRC20 materialized view
CREATE OR REPLACE FUNCTION refresh_trc20_sync_cursor()
RETURNS TRIGGER AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY trc20_sync_cursor;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Trigger for ERC20 token transfers on INSERT
CREATE TRIGGER trg_erc20_transfer_insert_refresh_cursor
AFTER INSERT ON erc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_erc20_sync_cursor();

-- Trigger for ERC20 token transfers on UPDATE of blockchain_confirmed
CREATE TRIGGER trg_erc20_transfer_update_refresh_cursor
AFTER UPDATE OF blockchain_confirmed ON erc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_erc20_sync_cursor();

-- Trigger for TRC20 token transfers on INSERT
CREATE TRIGGER trg_trc20_transfer_insert_refresh_cursor
AFTER INSERT ON trc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_trc20_sync_cursor();

-- Trigger for TRC20 token transfers on UPDATE of blockchain_confirmed
CREATE TRIGGER trg_trc20_transfer_update_refresh_cursor
AFTER UPDATE OF blockchain_confirmed ON trc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_trc20_sync_cursor();
