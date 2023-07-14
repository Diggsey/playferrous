use std::convert::Infallible;

use aerosol::{Aerosol, AsyncConstructible};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use thiserror::Error;

pub mod proposal;
pub mod session;

pub type PgTransaction = Transaction<'static, Postgres>;

#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

#[async_trait]
impl AsyncConstructible for Database {
    type Error = sqlx::Error;
    async fn construct_async(_aero: &Aerosol) -> Result<Self, Self::Error> {
        let url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        Ok(Database { pool })
    }
}

impl Database {
    pub async fn begin(&self) -> Result<PgTransaction, sqlx::Error> {
        self.pool.begin().await
    }
}

#[derive(Debug, Error)]
pub enum TransactError<A = Infallible> {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    App(A),
}

pub fn convert_error<A: From<anyhow::Error>>(e: TransactError<A>) -> A {
    match e {
        TransactError::App(e) => e,
        TransactError::Sqlx(e) => A::from(e.into()),
    }
}

macro_rules! transact {
    ($err:ty, $aero:expr, |$tx:ident| $expr:expr) => {{
        async {
            let db = $aero
                .try_obtain_async::<$crate::database::Database>()
                .await?;
            let mut tx = db.begin().await?;
            let $tx = &mut tx;
            match async { $expr }.await {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => Err(e),
            }
        }
        .await
        .map_err($crate::database::convert_error::<$err>)
    }};
}
