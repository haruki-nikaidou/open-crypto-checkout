use crate::entities::StablecoinName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erc20PendingDeposit {
    pub id: i64,
    pub order: uuid::Uuid,
    pub token_name: StablecoinName,
    pub chain: EtherScanChain,
    pub user_address: Option<String>,
    pub wallet_address: String,
    pub value: rust_decimal::Decimal,
    pub started_at: time::PrimitiveDateTime,
    pub last_scanned_at: time::PrimitiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(rename_all = "lowercase", type_name = "etherscan_chain")]
/// https://docs.etherscan.io/supported-chains
pub enum EtherScanChain {
    Ethereum = 1,
    Polygon = 137,
    Base = 8453,
    ArbitrumOne = 42161,
    Linea = 59144,
    Optimism = 10,
    AvalancheC = 43114,
}

impl From<EtherScanChain> for ocrch_sdk::objects::blockchains::Blockchain {
    fn from(value: EtherScanChain) -> Self {
        match value {
            EtherScanChain::Ethereum => ocrch_sdk::objects::blockchains::Blockchain::Ethereum,
            EtherScanChain::Polygon => ocrch_sdk::objects::blockchains::Blockchain::Polygon,
            EtherScanChain::Base => ocrch_sdk::objects::blockchains::Blockchain::Base,
            EtherScanChain::ArbitrumOne => ocrch_sdk::objects::blockchains::Blockchain::ArbitrumOne,
            EtherScanChain::Linea => ocrch_sdk::objects::blockchains::Blockchain::Linea,
            EtherScanChain::Optimism => ocrch_sdk::objects::blockchains::Blockchain::Optimism,
            EtherScanChain::AvalancheC => ocrch_sdk::objects::blockchains::Blockchain::AvalancheC,
        }
    }
}

impl serde::Serialize for EtherScanChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&(*self as i64).to_string())
    }
}

impl<'de> serde::Deserialize<'de> for EtherScanChain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value: i64 = s.parse().map_err(serde::de::Error::custom)?;
        match value {
            1 => Ok(EtherScanChain::Ethereum),
            137 => Ok(EtherScanChain::Polygon),
            8453 => Ok(EtherScanChain::Base),
            42161 => Ok(EtherScanChain::ArbitrumOne),
            59144 => Ok(EtherScanChain::Linea),
            10 => Ok(EtherScanChain::Optimism),
            43114 => Ok(EtherScanChain::AvalancheC),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["1", "137", "8453", "42161", "59144", "10", "43114"],
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erc20PendingDepositInsert {
    pub order: uuid::Uuid,
    pub token_name: StablecoinName,
    pub chain: EtherScanChain,
    pub user_address: Option<String>,
    pub wallet_address: String,
    pub value: rust_decimal::Decimal,
}

/// A pending deposit for matching operations.
#[derive(Debug, Clone)]
pub struct Erc20PendingDepositMatch {
    pub id: i64,
    pub order_id: uuid::Uuid,
    pub wallet_address: String,
    pub value: rust_decimal::Decimal,
    pub started_at_timestamp: i64,
}

impl Erc20PendingDeposit {
    /// Insert a new pending deposit.
    pub async fn insert_new(
        pool: &sqlx::PgPool,
        insert: Erc20PendingDepositInsert,
    ) -> Result<Erc20PendingDeposit, sqlx::Error> {
        let deposit = sqlx::query_as!(
            Erc20PendingDeposit,
            r#"
            INSERT INTO erc20_pending_deposits ("order", token_name, chain, user_address, wallet_address, value)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING 
            id,
            "order",
            token_name as "token_name: StablecoinName",
            chain as "chain: EtherScanChain",
            user_address,
            wallet_address,
            value,
            started_at,
            last_scanned_at
            "#,
            insert.order,
            insert.token_name as StablecoinName,
            insert.chain as EtherScanChain,
            insert.user_address as Option<String>,
            insert.wallet_address as String,
            insert.value,
        )
        .fetch_one(pool)
        .await?;
        Ok(deposit)
    }

    /// Get pending deposits for matching with transfers.
    pub async fn get_for_matching(
        pool: &sqlx::PgPool,
        chain: EtherScanChain,
        token: StablecoinName,
    ) -> Result<Vec<Erc20PendingDepositMatch>, sqlx::Error> {
        let deposits = sqlx::query_as!(
            Erc20PendingDepositMatch,
            r#"
            SELECT 
                d.id,
                d."order" as order_id,
                d.wallet_address,
                d.value,
                EXTRACT(EPOCH FROM d.started_at)::bigint as "started_at_timestamp!"
            FROM erc20_pending_deposits d
            JOIN order_records o ON d."order" = o.order_id
            WHERE d.chain = $1 
              AND d.token_name = $2
              AND o.status = 'pending'
            "#,
            chain as EtherScanChain,
            token as StablecoinName,
        )
        .fetch_all(pool)
        .await?;
        Ok(deposits)
    }

    /// Delete pending deposits for an order except for one (the matched one), within a transaction.
    pub async fn delete_for_order_except_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: uuid::Uuid,
        except_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM erc20_pending_deposits
            WHERE "order" = $1 AND id != $2
            "#,
            order_id,
            except_id,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Delete all pending deposits for an order within a transaction.
    pub async fn delete_for_order_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM erc20_pending_deposits
            WHERE "order" = $1
            "#,
            order_id,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
