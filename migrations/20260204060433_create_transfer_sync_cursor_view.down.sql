-- Drop triggers first
DROP TRIGGER IF EXISTS trg_erc20_transfer_insert_refresh_cursor ON erc20_token_transfers;
DROP TRIGGER IF EXISTS trg_erc20_transfer_update_refresh_cursor ON erc20_token_transfers;
DROP TRIGGER IF EXISTS trg_trc20_transfer_insert_refresh_cursor ON trc20_token_transfers;
DROP TRIGGER IF EXISTS trg_trc20_transfer_update_refresh_cursor ON trc20_token_transfers;

-- Drop the refresh function
DROP FUNCTION IF EXISTS refresh_transfer_sync_cursor();

-- Drop the materialized view
DROP MATERIALIZED VIEW IF EXISTS transfer_sync_cursor;
