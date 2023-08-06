use std::{
    fmt::{self, Debug},
    future::Future,
};

use aerosol::Aero;
use async_trait::async_trait;
use sqlx::Postgres;

use super::{Database, TransactError};

#[async_trait]
pub trait TransactionHook: Send + 'static {
    async fn post_commit(self: Box<Self>, aero: &Aero) -> anyhow::Result<()>;
}

#[async_trait]
impl<F, R> TransactionHook for F
where
    F: FnOnce(Aero) -> R + Send + 'static,
    R: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    async fn post_commit(self: Box<Self>, aero: &Aero) -> anyhow::Result<()> {
        self(aero.clone()).await
    }
}

pub struct Transaction {
    aero: Aero,
    inner: sqlx::Transaction<'static, Postgres>,
    hooks: Vec<Box<dyn TransactionHook>>,
}

impl Debug for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transaction")
            .field("aero", &self.aero)
            .field("inner", &self.inner)
            .field("hook_count", &self.hooks.len())
            .finish()
    }
}

impl Transaction {
    pub async fn begin(aero: &Aero) -> Result<Self, sqlx::Error> {
        let db = aero.try_obtain_async::<Database>().await?;
        let inner = db.pool.begin().await?;
        Ok(Self {
            aero: aero.clone(),
            inner,
            hooks: Vec::new(),
        })
    }
    pub fn on_commit(&mut self, hook: impl TransactionHook) {
        self.hooks.push(Box::new(hook));
    }
    pub async fn commit<A>(self) -> Result<(), TransactError<A>> {
        self.inner.commit().await?;
        for hook in self.hooks {
            hook.post_commit(&self.aero).await?;
        }
        Ok(())
    }
}

impl<'c> sqlx::Executor<'c> for &'c mut Transaction {
    type Database = Postgres;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> futures::stream::BoxStream<
        'e,
        Result<
            sqlx::Either<
                <Self::Database as sqlx::Database>::QueryResult,
                <Self::Database as sqlx::Database>::Row,
            >,
            sqlx::Error,
        >,
    >
    where
        'c: 'e,
        E: sqlx::Execute<'q, Self::Database>,
    {
        self.inner.fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> futures::future::BoxFuture<
        'e,
        Result<Option<<Self::Database as sqlx::Database>::Row>, sqlx::Error>,
    >
    where
        'c: 'e,
        E: sqlx::Execute<'q, Self::Database>,
    {
        self.inner.fetch_optional(query)
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as sqlx::Database>::TypeInfo],
    ) -> futures::future::BoxFuture<
        'e,
        Result<<Self::Database as sqlx::database::HasStatement<'q>>::Statement, sqlx::Error>,
    >
    where
        'c: 'e,
    {
        self.inner.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> futures::future::BoxFuture<'e, Result<sqlx::Describe<Self::Database>, sqlx::Error>>
    where
        'c: 'e,
    {
        self.inner.describe(sql)
    }
}
