-- Enum Types
CREATE TYPE stablecoin_name AS ENUM ('usdt', 'usdc', 'dai');

CREATE TYPE order_status AS ENUM ('pending', 'paid', 'expired', 'cancelled');

CREATE TYPE transfer_status AS ENUM (
    'waiting_for_confirmation',
    'failed_to_confirm',
    'waiting_for_match',
    'no_matched_deposit',
    'matched'
);

CREATE TYPE etherscan_chain AS ENUM (
    'ethereum',
    'polygon',
    'base',
    'arbitrum_one',
    'linea',
    'optimism',
    'avalanche_c'
);

-- Order Records Table
CREATE TABLE order_records (
    order_id UUID PRIMARY KEY,
    merchant_order_id TEXT NOT NULL,
    amount NUMERIC NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status order_status NOT NULL DEFAULT 'pending',
    webhook_success_at TIMESTAMP,
    webhook_url TEXT NOT NULL,
    webhook_retry_count INTEGER NOT NULL DEFAULT 0 CHECK (webhook_retry_count >= 0),
    webhook_last_tried_at TIMESTAMP
);

CREATE INDEX idx_order_records_merchant_order_id ON order_records (merchant_order_id);
CREATE INDEX idx_order_records_status ON order_records (status);

-- ERC20 Pending Deposits Table
CREATE TABLE erc20_pending_deposits (
    id BIGSERIAL PRIMARY KEY,
    "order" UUID NOT NULL REFERENCES order_records (order_id) ON DELETE CASCADE,
    token_name stablecoin_name NOT NULL,
    chain etherscan_chain NOT NULL,
    user_address TEXT,
    wallet_address TEXT NOT NULL,
    value NUMERIC NOT NULL,
    started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_scanned_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_erc20_pending_deposits_order ON erc20_pending_deposits ("order");
CREATE INDEX idx_erc20_pending_deposits_wallet ON erc20_pending_deposits (wallet_address);

-- TRC20 Pending Deposits Table
CREATE TABLE trc20_pending_deposits (
    id BIGSERIAL PRIMARY KEY,
    "order" UUID NOT NULL REFERENCES order_records (order_id) ON DELETE CASCADE,
    token_name stablecoin_name NOT NULL,
    user_address TEXT,
    wallet_address TEXT NOT NULL,
    value NUMERIC NOT NULL,
    started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_scanned_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_trc20_pending_deposits_order ON trc20_pending_deposits ("order");
CREATE INDEX idx_trc20_pending_deposits_wallet ON trc20_pending_deposits (wallet_address);

-- ERC20 Token Transfers Table
CREATE TABLE erc20_token_transfers (
    id BIGSERIAL PRIMARY KEY,
    token_name stablecoin_name NOT NULL,
    chain etherscan_chain NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    txn_hash TEXT NOT NULL,
    value NUMERIC NOT NULL,
    block_number BIGINT NOT NULL CHECK (block_number >= 0),
    block_timestamp BIGINT NOT NULL CHECK (block_timestamp >= 0),
    blockchain_confirmed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status transfer_status NOT NULL DEFAULT 'waiting_for_confirmation',
    fulfillment_id BIGINT
);

CREATE UNIQUE INDEX idx_erc20_token_transfers_txn_hash ON erc20_token_transfers (txn_hash, chain);
CREATE INDEX idx_erc20_token_transfers_to_address ON erc20_token_transfers (to_address);
CREATE INDEX idx_erc20_token_transfers_status ON erc20_token_transfers (status);

-- TRC20 Token Transfers Table
CREATE TABLE trc20_token_transfers (
    id BIGSERIAL PRIMARY KEY,
    token_name stablecoin_name NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    txn_hash TEXT NOT NULL UNIQUE,
    value NUMERIC NOT NULL,
    block_number BIGINT NOT NULL CHECK (block_number >= 0),
    block_timestamp BIGINT NOT NULL CHECK (block_timestamp >= 0),
    blockchain_confirmed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status transfer_status NOT NULL DEFAULT 'waiting_for_confirmation',
    fulfillment_id BIGINT
);

CREATE INDEX idx_trc20_token_transfers_to_address ON trc20_token_transfers (to_address);
CREATE INDEX idx_trc20_token_transfers_status ON trc20_token_transfers (status);
