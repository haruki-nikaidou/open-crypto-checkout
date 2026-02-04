## Architecture

The project should be a monolithic event driven system.

There are 3 crates:

- `ocrch-sdk`: The SDK crate that define the API and implement 4 kinds of clients for different set of APIs. This will be published to `crates.io`
- `ocrch-core`: Critical business login crate. This won't be published but will be used in `ocrch-server`.
- `ocrch-server`: The executable binary.

All events in the system should be idempotent and expected to be ephemeral. All event pipes should be implemented in tokio stream.

The event flows should be:

1. Every time a pending deposit changes its status, it should emit `PendingDepositChanged` event.
2. `PoolingManager` is and is the only receiver of `PendingDepositChanged` event, it will re-calculate the frequency of pooling for that blockchain-token pair.
3. `PoolingManage` will emit `PoolingTick` event circularly. The `PoolingTick` event will be received by `BlockchainSync`, who fetch the data from blockchain explorer API and write it into database (should use single `INSERT` query and make sure the txn hash never duplicated). Each enabled token in each blockchain will have their own `BlockchainSync`.
4. Every time a `BlockchainSync` receives a `PoolingTick` event, it emits a `MatchTick` to `OrderBookWatcher`, which will try to match all pending deposit of all pending orders to transfers in the time window.
5. All webhook events should be send by `WebhookSender`, which receive events from otherwhere.

## Service Implementation

The executable binary must be based on tokio to handle async, steaming processing and system events. Assume the project will only be ran on Linux or other unix-like systems, not including Windows.

It read database URL from environment variable and all other config form config file `./ocrch-config.toml`. Use `crap` to allow it override the config name and path.

For graceful shutdown, when a shutdown signal is received, it will immediately shutdown API and all threads that doesn't receive an event, then disallow new event to be received and wait for the latest event handling.

It should support config reload without rebooting the service.

Merchant should be stored as plaintext in the config file, but the admin secret must be stored with argon2 protected. When the password in config file is not a valid argon2 hash result string, consider it as a plaintext and rewrite it with argon2 hash.

## Functionality

This headless cryptocurrency checkout counter work like this:

1. The application backend server calls checkout's API to create a pending order and get the order ID. An initial blockchain+stablecoin pair is optional. This creating API will return the new order back to the application backend.
2. Then, the application backend server build a signed url towards the checkout counter frontend (which is not included here and require merchant to ship their own). The user are expected to redirect to the signed URL to finish the payment in counter frontend. The frontend URl signature algorithm is {HMAC(FULL_URL+"."+CURRENT_TIMESTAMP)}. The FULL_URL can be any URL, as long as the frontend can work well.
3. The user select the chain and stablecoin to get the payment address. Every time the user select a new chain, it generate a new pending deposit for the order. If one of the deposit is fulfilled, all pending deposits are deleted excluding the fulfilled one. When the user is paying, the frontend should open a websocket to the headless backend to monitor the payment status.
4. After payment succeeded, the user will be redirected to URL by frontend.

There should be 4 sets of APIs:

1. Service API: The API called by application backend, including the API that allows the application backend to create the order and to get the order status of a single order. These 2 API return same thing. These APIs require a signed body.
2. User API: The API called by user's browser. These APIs require a verified domain and a correctly signed frontend URL. It includes:
    a. get available chain-coin pair list
    b. get payment detail (which could create new pending deposit)
    c. cancel the order
    d. get order status (pooling version)
    e. get order status (ws version)
3. Webhook API: The API called by counter to notify the application backend, audit system or customer manage system. These APIs require a signed body. It includes:
    a. order status change event: required. Notify the order is paid, expired or cancelled
    b. unknown payment received event: optional. Notify a transaction for unknown purpose is received.
4. Admin API: The API called by admin dashboard frontend. Unless other API, this API don't use merchant secret, but admin secret. The admin secret should be included as plain text in "Ocrch-Admin-Authorization" header. It includes:
    a. list all orders, with limit (default to 1) and offset (default to 0), and optional filter options
    b. list all pending deposit, with limit (default to 1) and offset (default to 0), and optional filter options
    c. list all transfers, with required wallet address in path, limit (default to 1), offset (default to 0) and other optional filter options
    d. show wallets and enabled coins.
    e. mark a order as paid whatsoever
    f. resend order status event manually
    g. resend unknown transfer event manually

For all these API mentioned, excluding admin API, the signature should be in "Ocrch-Signature".

When the remote returns `500 OK`, the webhook is considered to be successfully handled by remote. If failed, retry it after 2^0(1)~2^11(2048) seconds. A table for webhook resend may be needed.