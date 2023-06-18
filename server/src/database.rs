use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect() -> Result<Self, sqlx::Error> {
        let url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        Ok(Database { pool })
    }

    pub async fn begin(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }
}

#[derive(Debug, Error)]
pub enum TransactError<A> {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    App(A),
}

pub fn convert_error<A: From<anyhow::Error>, E: Into<TransactError<A>>>(e: E) -> A {
    match e.into() {
        TransactError::App(e) => e,
        TransactError::Sqlx(e) => A::from(e.into()),
    }
}

macro_rules! transact {
    ($db:expr, |$tx:ident| $expr:expr) => {{
        let mut tx = match $db.begin().await {
            Ok(v) => v,
            Err(e) => return Err($crate::database::convert_error(e)),
        };
        let $tx = &mut tx;
        match async move { $expr }.await {
            Ok(v) => {
                match tx.commit().await {
                    Ok(()) => {}
                    Err(e) => return Err($crate::database::convert_error(e)),
                };
                Ok(v)
            }
            Err(e) => Err($crate::database::convert_error::<_, TransactError<_>>(e)),
        }
    }};
}
