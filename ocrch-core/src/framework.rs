use sqlx::PgPool;

pub struct DatabaseProcessor {
    pub pool: PgPool,
}
