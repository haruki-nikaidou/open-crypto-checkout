use sqlx::PgPool;

pub trait DatabaseAccessor {
    fn acquire(&mut self) -> impl sqlx::PgExecutor<'_>;
}

pub struct DatabaseProcessor {
    pub pool: PgPool,
}

pub struct TransactionProcessor<'b> {
    pub tx: sqlx::Transaction<'b, sqlx::Postgres>,
}

impl DatabaseAccessor for DatabaseProcessor {
    fn acquire(&mut self) -> impl sqlx::PgExecutor<'_> {
        &self.pool
    }
}

impl<'b> DatabaseAccessor for TransactionProcessor<'b> {
    fn acquire(&mut self) -> impl sqlx::PgExecutor<'_> {
        &mut *self.tx
    }
}
