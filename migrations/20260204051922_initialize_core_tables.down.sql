-- Drop tables (in reverse order of creation due to foreign key constraints)
DROP TABLE IF EXISTS trc20_token_transfers;
DROP TABLE IF EXISTS erc20_token_transfers;
DROP TABLE IF EXISTS trc20_pending_deposits;
DROP TABLE IF EXISTS erc20_pending_deposits;
DROP TABLE IF EXISTS order_records;

-- Drop enum types
DROP TYPE IF EXISTS etherscan_chain;
DROP TYPE IF EXISTS transfer_status;
DROP TYPE IF EXISTS order_status;
DROP TYPE IF EXISTS stablecoin_name;
