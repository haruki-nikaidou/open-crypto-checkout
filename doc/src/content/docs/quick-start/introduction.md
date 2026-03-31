---
title: Introduction
description: Learn the core concepts behind Open Crypto Checkout before you deploy.
sidebar:
  order: 1
---

Open Crypto Checkout (Ocrch) is a **headless, self-hosted payment gateway** for stablecoin payments. It provides an HTTP API server that your application backend and checkout frontend talk to — no bundled UI is included. You ship the checkout page; Ocrch handles the on-chain tracking, order management, and webhook delivery.

## Core Concepts

### Orders

An **order** represents a single payment intent created by your application backend. Orders are identified by two IDs:

- **`order_id`** — an internal UUID assigned by Ocrch when the order is created.
- **`merchant_order_id`** — an opaque string you supply (e.g. your e-commerce order number). Ocrch stores it for correlation but never interprets it.

An order can have one of four statuses:

| Status | Meaning |
|--------|---------|
| `pending` | Awaiting payment |
| `paid` | Successfully paid on-chain |
| `expired` | Timed out before payment was received |
| `cancelled` | Cancelled by the user or merchant |

### Pending Deposits

When a user selects a blockchain and stablecoin on your checkout page, Ocrch creates a **pending deposit** — a record saying "I expect `X` of `TOKEN` to arrive at `WALLET` on `CHAIN` for order `Y`." Multiple pending deposits can exist simultaneously for one order (one per chain/coin combination the user has explored). If any deposit is fulfilled, the others are cleaned up.

### Blockchain Sync

Ocrch polls blockchain explorer APIs (Etherscan-compatible APIs for EVM chains, Tronscan for Tron) on an adaptive schedule. When a transfer is detected that matches a pending deposit, the order is marked `paid` and a webhook is fired.

### Webhooks

Ocrch delivers signed HTTP POST requests to your backend when order status changes or when an unrecognized transfer arrives at one of your wallets. Webhooks are signed with the same HMAC key used for the Service API, so your backend can verify authenticity.

### API Surfaces

There are four distinct API groups:

| API | Caller | Auth |
|-----|--------|------|
| **Service API** | Your application backend | HMAC-signed JSON body |
| **User API** | The checkout frontend (browser) | HMAC-signed URL |
| **Admin API** | Admin dashboard | Plaintext admin secret header |
| **Webhooks** | Ocrch → your backend | HMAC-signed JSON body |

## Payment Flow

Here is the end-to-end flow for a successful payment:

```
1. Your backend  →  POST /api/v1/service/orders
                    (creates order, returns order_id)

2. Your backend builds a signed checkout URL:
   https://checkout.example.com/pay?order_id=<uuid>
   and redirects the user to it.

3. User lands on YOUR checkout page (you build this).

4. Checkout page  →  GET /api/v1/user/chains
                     (lists available chain/coin options)

5. User picks Ethereum + USDT.
   Checkout page  →  POST /api/v1/user/orders/<id>/payment
                     (creates pending deposit, returns wallet address)

6. Checkout page opens WebSocket to /api/v1/user/orders/<id>/ws
   and displays the wallet address + expected amount to the user.

7. User sends USDT to the wallet address on Ethereum.

8. Ocrch detects the transfer via Etherscan API, matches it to the
   pending deposit, marks the order as `paid`, and fires a webhook
   to your backend.

9. WebSocket pushes `status_update` with status `paid` to the
   checkout page. The page redirects the user to your success URL.
```

## Supported Blockchains & Stablecoins

| Chain | Identifier | Supported Stablecoins |
|-------|------------|-----------------------|
| Ethereum | `eth` | USDT, USDC, DAI |
| Polygon | `polygon` | USDT, USDC, DAI |
| Base | `base` | USDC, DAI |
| Arbitrum One | `arb` | USDC, DAI |
| Optimism | `op` | USDC, DAI |
| Linea | `linea` | USDC, DAI |
| Avalanche C-Chain | `avaxc` | USDC, DAI |
| Tron | `tron` | USDT |

:::note
Not all stablecoins are available on all chains due to on-chain liquidity and contract availability. The User API `/chains` endpoint always returns the real-time list of active chain/coin pairs based on your wallet configuration.
:::

## What You Need to Build

Ocrch is intentionally headless. You are responsible for:

1. **Your checkout frontend** — a web page the user visits to complete payment. It calls the User API. See the [Frontend Development guide](/guides/frontend/) for details.
2. **Your application backend** — creates orders via the Service API and handles incoming webhooks.
3. **Wallet management** — you must provide dedicated wallet addresses in the config. Use wallets that are solely dedicated to Ocrch; the blockchain sync tracks all incoming transfers to those addresses.

## Next Steps

- [Deploy with Docker](/quick-start/docker/) — get Ocrch running in minutes.
- [Configuration](/guides/configuration/) — configure wallets, secrets, and API keys.
- [Frontend Development](/guides/frontend/) — build your checkout page.
