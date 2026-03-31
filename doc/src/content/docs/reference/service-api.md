---
title: Service API
description: API reference for the Service API — called by your application backend to manage orders.
sidebar:
  order: 2
---

The Service API is called by your **application backend** to create payment orders and query their status. All requests must be authenticated with a signed JSON body.

**Base path:** `/api/v1/service`  
**Authentication:** `Ocrch-Signature` header — see [Authentication](/reference/authentication/#service-api--signed-json-body)

---

## Endpoints

### `POST /orders`

Create a new pending payment order.

**Request headers:**

| Header | Value |
|--------|-------|
| `Content-Type` | `application/json` |
| `Ocrch-Signature` | `{timestamp}.{base64_hmac}` |

**Request body:**

```json
{
  "order_id": "your-order-reference-123",
  "amount": "19.99",
  "webhook_url": "https://your-app.example.com/webhooks/ocrch",
  "expecting_wallet_address": null,
  "blockchain": null,
  "stablecoin": null
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `order_id` | string | Yes | Your merchant-assigned order identifier. Stored as-is; not interpreted by Ocrch. |
| `amount` | decimal string | Yes | Payment amount in the stablecoin's base unit (e.g. `"19.99"` for $19.99 USDT). |
| `webhook_url` | string | Yes | URL that Ocrch will POST webhook events to when the order status changes. |
| `expecting_wallet_address` | string \| null | No | If set, Ocrch will only match transfers originating from this address. |
| `blockchain` | string \| null | No | Pre-select a blockchain (e.g. `"eth"`). The user cannot change it on the checkout page. |
| `stablecoin` | string \| null | No | Pre-select a stablecoin (e.g. `"USDT"`). The user cannot change it on the checkout page. |

**Response — `201 Created`:**

```json
{
  "order_id": "550e8400-e29b-41d4-a716-446655440000",
  "merchant_order_id": "your-order-reference-123",
  "amount": "19.99",
  "status": "pending",
  "created_at": 1711900800
}
```

| Field | Type | Description |
|-------|------|-------------|
| `order_id` | UUID string | Internal Ocrch ID — use this to build the signed checkout URL. |
| `merchant_order_id` | string | Echoed back from your `order_id` field. |
| `amount` | decimal string | Payment amount. |
| `status` | string | Always `"pending"` for newly created orders. |
| `created_at` | integer | Unix timestamp of order creation. |

**Blockchain identifiers:**

| Value | Chain |
|-------|-------|
| `"eth"` | Ethereum |
| `"polygon"` | Polygon |
| `"base"` | Base |
| `"arb"` | Arbitrum One |
| `"op"` | Optimism |
| `"linea"` | Linea |
| `"avaxc"` | Avalanche C-Chain |
| `"tron"` | Tron |

**Stablecoin identifiers:**

| Value | Coin |
|-------|------|
| `"USDT"` | Tether |
| `"USDC"` | USD Coin |
| `"DAI"` | Dai |

**Example (TypeScript):**

```ts
import { createHmac } from "node:crypto";

async function createOrder(orderId: string, amount: string, webhookUrl: string) {
  const payload = {
    order_id: orderId,
    amount,
    webhook_url: webhookUrl,
    expecting_wallet_address: null,
    blockchain: null,
    stablecoin: null,
  };

  const timestamp = Math.floor(Date.now() / 1000).toString();
  const json = JSON.stringify(payload);
  const sig = createHmac("sha256", process.env.MERCHANT_SECRET!)
    .update(`${timestamp}.${json}`)
    .digest("base64");

  const res = await fetch("https://checkout-api.example.com/api/v1/service/orders", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Ocrch-Signature": `${timestamp}.${sig}`,
    },
    body: json,
  });

  if (!res.ok) throw new Error(await res.text());
  return res.json();
}
```

---

### `POST /orders/status`

Get the current status of an existing order.

**Request headers:**

| Header | Value |
|--------|-------|
| `Content-Type` | `application/json` |
| `Ocrch-Signature` | `{timestamp}.{base64_hmac}` |

**Request body:**

```json
{
  "order_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `order_id` | UUID string | Yes | The internal Ocrch order ID returned when the order was created. |

**Response — `200 OK`:**

Same structure as the create order response.

```json
{
  "order_id": "550e8400-e29b-41d4-a716-446655440000",
  "merchant_order_id": "your-order-reference-123",
  "amount": "19.99",
  "status": "paid",
  "created_at": 1711900800
}
```

**Error responses:**

| Status | Body | Cause |
|--------|------|-------|
| `401 Unauthorized` | `missing Ocrch-Signature header` | Missing or invalid auth |
| `401 Unauthorized` | `signature verification failed` | HMAC mismatch |
| `404 Not Found` | `order not found` | No order with that UUID |
| `500 Internal Server Error` | `internal server error` | Database error |

---

## Full Request/Response Flow

```
Backend                          Ocrch Server
  |                                  |
  |  POST /api/v1/service/orders     |
  |  Ocrch-Signature: ts.hmac        |
  |  { order_id, amount, ... }       |
  | ───────────────────────────────> |
  |                                  | Verify HMAC
  |                                  | Insert order (status=pending)
  |  201 Created                     |
  |  { order_id: UUID, ... }         |
  | <─────────────────────────────── |
  |                                  |
  |  (build signed checkout URL)     |
  |  (redirect user to checkout)     |
  |                                  |
  |  POST /api/v1/service/orders/status
  |  { order_id: UUID }              |
  | ───────────────────────────────> |
  |  200 OK { status: "paid", ... }  |
  | <─────────────────────────────── |
```
