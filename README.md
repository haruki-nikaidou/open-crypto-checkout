# Open Crypto Checkout

A self-hosted, headless cryptocurrency checkout server for accepting stablecoin payments. Your backend creates orders via a signed API; your frontend handles the payment UI and talks directly to the checkout server via a signed URL.

## Features

- **Headless** — no bundled frontend; bring your own checkout UI
- **Stablecoin focused** — USDT, USDC, and DAI across multiple chains
- **Multi-chain** — Ethereum, Polygon, Arbitrum One, Base, Optimism, Linea, Avalanche C-Chain, and Tron
- **Webhook delivery** — reliable order-status and unknown-payment notifications with exponential-backoff retries
- **Real-time status** — WebSocket endpoint for live payment monitoring in the browser
- **Signed API** — HMAC-SHA256 request signing for the service and webhook APIs
- **Admin dashboard API** — manage orders, deposits, transfers, and wallets; manually mark orders paid or resend webhooks
- **Docker-ready** — distroless production image, single binary, no runtime dependencies besides PostgreSQL

## How It Works

1. Your backend calls the **Service API** to create a pending order and receives an order ID.
2. Your backend generates a signed URL pointing to your frontend checkout page.
3. The user lands on your frontend, selects a chain and stablecoin, and is shown a wallet address. The frontend opens a WebSocket to monitor payment status.
4. The checkout server polls the relevant blockchain explorer, matches incoming transfers to pending deposits, and fires a webhook to your backend once payment is confirmed.

## Supported Chains and Stablecoins

| Chain | USDT | USDC | DAI |
|---|---|---|---|
| Ethereum | ✓ | ✓ | ✓ |
| Polygon | ✓ | ✓ | ✓ |
| Arbitrum One | ✓ | ✓ | ✓ |
| Base | | ✓ | |
| Optimism | ✓ | ✓ | ✓ |
| Linea | ✓ | ✓ | |
| Avalanche C-Chain | ✓ | ✓ | |
| Tron (TRC-20) | ✓ | | |

## Quick Start with Docker

```bash
# 1. Copy and edit the config file
cp ocrch-config.example.toml ocrch-config.toml

# 2. Run database migrations (requires sqlx-cli)
DATABASE_URL=postgres://user:pass@localhost/ocrch sqlx migrate run

# 3. Start the server
docker run \
  -e DATABASE_URL=postgres://user:pass@localhost/ocrch \
  -v $(pwd)/ocrch-config.toml:/app/ocrch-config.toml:ro \
  -p 8080:8080 \
  ghcr.io/haruki-nikaidou/open-crypto-checkout:latest
```

See the [documentation site](https://ocrch.suitsu31.club/) for a full deployment guide.

## Configuration

Copy `ocrch-config.example.toml` to `ocrch-config.toml` and fill in your values:

```toml
[server]
listen = "0.0.0.0:8080"

[admin]
secret = "your-admin-password"   # hashed to Argon2 on first run

[merchant]
name = "My Store"
secret = "your-merchant-secret"  # used for HMAC-SHA256 request signing
allowed_origins = ["https://checkout.example.com"]

[[wallets]]
blockchain = "eth"
address = "0xYourAddress"
enabled_coins = ["USDT", "USDC"]
```

The database URL is read from the `DATABASE_URL` environment variable.

## Project Structure

```
open-crypto-checkout/
├── ocrch-core/        # Business logic (event-driven, not published)
├── ocrch-sdk/         # Wire types and HTTP clients (published to crates.io)
├── ocrch-server/      # Executable binary (Axum HTTP server)
│   └── Dockerfile
├── migrations/        # PostgreSQL migrations (sqlx)
├── doc/               # Documentation site (Astro + Starlight)
└── ocrch-config.example.toml
```

## Building from Source

Requires Rust (see `rust-toolchain.toml`) and a running PostgreSQL instance.

```bash
# Offline sqlx — query metadata is pre-generated in .sqlx/
SQLX_OFFLINE=true cargo build --release --bin ocrch-server
```

## SDK

`ocrch-sdk` provides typed Rust clients for all four API surfaces (Service, User, Admin, Webhook verification). Enable the `client` feature to include the HTTP clients:

```toml
[dependencies]
ocrch-sdk = { version = "0.1", features = ["client"] }
```

## License

Apache License 2.0 — see [LICENSE](LICENSE).
