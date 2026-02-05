-- Drop triggers first
DROP TRIGGER IF EXISTS trg_erc20_transfer_insert_refresh_cursor ON erc20_token_transfers;
DROP TRIGGER IF EXISTS trg_erc20_transfer_update_refresh_cursor ON erc20_token_transfers;
DROP TRIGGER IF EXISTS trg_trc20_transfer_insert_refresh_cursor ON trc20_token_transfers;
DROP TRIGGER IF EXISTS trg_trc20_transfer_update_refresh_cursor ON trc20_token_transfers;

-- Drop the refresh functions
DROP FUNCTION IF EXISTS refresh_erc20_sync_cursor();
DROP FUNCTION IF EXISTS refresh_trc20_sync_cursor();

-- Drop the materialized views
DROP MATERIALIZED VIEW IF EXISTS erc20_sync_cursor;
DROP MATERIALIZED VIEW IF EXISTS trc20_sync_cursor;
