use std::convert::Infallible;

use aerosol::{Aero, AsyncConstructible};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use thiserror::Error;

pub mod message;
pub mod proposal;
pub mod session;
pub mod transaction;
pub mod user;

#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

#[async_trait]
impl AsyncConstructible for Database {
    type Error = sqlx::Error;
    async fn construct_async(_aero: &Aero) -> Result<Self, Self::Error> {
        let url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        Ok(Database { pool })
    }
}

#[derive(Debug, Error)]
pub enum TransactError<A = Infallible> {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    App(A),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub fn convert_error<A: From<anyhow::Error>>(e: TransactError<A>) -> A {
    match e {
        TransactError::App(e) => e,
        TransactError::Sqlx(e) => A::from(e.into()),
        TransactError::Internal(e) => A::from(e),
    }
}

macro_rules! transact {
    ($err:ty, $aero:expr, |$tx:ident| $expr:expr) => {{
        async {
            let mut tx = crate::database::transaction::Transaction::begin(&$aero).await?;
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
