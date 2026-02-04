-- Create materialized view for transfer sync cursor
-- Cursor algorithm:
-- 1. If there are unconfirmed transfers within the last 1 day, use the earliest unconfirmed transfer
-- 2. Otherwise, use the latest transfer
-- 3. If no transfers exist, return empty result

CREATE MATERIALIZED VIEW transfer_sync_cursor AS
WITH one_day_ago AS (
    SELECT EXTRACT(EPOCH FROM (NOW() - INTERVAL '1 day'))::BIGINT AS ts
),
-- ERC20 transfers cursor per chain
erc20_cursors AS (
    SELECT
        chain::TEXT AS network,
        token_name,
        CASE
            -- If there are unconfirmed transfers within the last 1 day, use the earliest
            WHEN EXISTS (
                SELECT 1 FROM erc20_token_transfers e2, one_day_ago
                WHERE e2.chain = e.chain
                  AND e2.token_name = e.token_name
                  AND e2.blockchain_confirmed = FALSE
                  AND e2.block_timestamp >= one_day_ago.ts
            ) THEN (
                SELECT MIN(e3.block_timestamp)
                FROM erc20_token_transfers e3, one_day_ago
                WHERE e3.chain = e.chain
                  AND e3.token_name = e.token_name
                  AND e3.blockchain_confirmed = FALSE
                  AND e3.block_timestamp >= one_day_ago.ts
            )
            -- Otherwise, use the latest transfer
            ELSE MAX(e.block_timestamp)
        END AS cursor_block_timestamp,
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
    GROUP BY chain, token_name
),
-- TRC20 transfers cursor (no chain distinction)
trc20_cursors AS (
    SELECT
        'tron'::TEXT AS network,
        token_name,
        CASE
            -- If there are unconfirmed transfers within the last 1 day, use the earliest
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
            -- Otherwise, use the latest transfer
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
    GROUP BY token_name
)
SELECT * FROM erc20_cursors
UNION ALL
SELECT * FROM trc20_cursors;

-- Create unique index for concurrent refresh
CREATE UNIQUE INDEX idx_transfer_sync_cursor_network_token
ON transfer_sync_cursor (network, token_name);

-- Function to refresh the materialized view
CREATE OR REPLACE FUNCTION refresh_transfer_sync_cursor()
RETURNS TRIGGER AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY transfer_sync_cursor;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Trigger for ERC20 token transfers on INSERT
CREATE TRIGGER trg_erc20_transfer_insert_refresh_cursor
AFTER INSERT ON erc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_transfer_sync_cursor();

-- Trigger for ERC20 token transfers on UPDATE of blockchain_confirmed
CREATE TRIGGER trg_erc20_transfer_update_refresh_cursor
AFTER UPDATE OF blockchain_confirmed ON erc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_transfer_sync_cursor();

-- Trigger for TRC20 token transfers on INSERT
CREATE TRIGGER trg_trc20_transfer_insert_refresh_cursor
AFTER INSERT ON trc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_transfer_sync_cursor();

-- Trigger for TRC20 token transfers on UPDATE of blockchain_confirmed
CREATE TRIGGER trg_trc20_transfer_update_refresh_cursor
AFTER UPDATE OF blockchain_confirmed ON trc20_token_transfers
FOR EACH STATEMENT
EXECUTE FUNCTION refresh_transfer_sync_cursor();
