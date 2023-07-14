use std::{any::Any, convert::Infallible, sync::Arc};

use aerosol::{Aerosol, Constructible};
use async_trait::async_trait;
use playferrous_presentation::{TerminalConnection, UserId, UserManagement, UserManagementError};
use sqlx::{Postgres, Transaction};

use crate::{database::TransactError, terminal_session::TerminalSession};

pub struct UserManagementImpl {
    aero: Aerosol,
}

impl Constructible for UserManagementImpl {
    type Error = Infallible;
    fn construct(aero: &Aerosol) -> Result<Self, Self::Error> {
        Ok(Self { aero: aero.clone() })
    }
    fn after_construction(
        this: &(dyn Any + Send + Sync),
        aero: &Aerosol,
    ) -> Result<(), Self::Error> {
        if let Some(arc) = this.downcast_ref::<Arc<Self>>() {
            aero.insert(arc.clone() as Arc<dyn UserManagement>)
        }
        Ok(())
    }
}

impl From<UserManagementError> for TransactError<UserManagementError> {
    fn from(value: UserManagementError) -> Self {
        Self::App(value)
    }
}

impl UserManagementImpl {
    async fn find_user_id(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        username: &str,
    ) -> Result<UserId, TransactError<UserManagementError>> {
        Ok(sqlx::query_scalar!(
            r#"
            SELECT id as "id: _" FROM "user"
            WHERE username = $1
            "#,
            username
        )
        .fetch_optional(tx)
        .await?
        .ok_or(UserManagementError::UserDoesNotExist)?)
    }
}

#[async_trait]
impl UserManagement for UserManagementImpl {
    async fn login_user_with_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserId, UserManagementError> {
        transact!(UserManagementError, self.aero, |tx| {
            let user_id = self.find_user_id(tx, username).await?;
            if sqlx::query!(
                r#"
                UPDATE "user"
                SET last_login_at = NOW()
                WHERE id = $1 AND password_hash = crypt($2, password_salt)
                "#,
                user_id as _,
                password
            )
            .execute(tx)
            .await?
            .rows_affected()
                == 1
            {
                Ok(user_id)
            } else {
                Err(UserManagementError::InvalidAuth.into())
            }
        })
    }
    async fn login_user_with_public_key(
        &self,
        username: &str,
        fingerprint: &str,
    ) -> Result<UserId, UserManagementError> {
        transact!(UserManagementError, self.aero, |tx| {
            let user_id = self.find_user_id(tx, username).await?;
            if sqlx::query!(
                r#"
                UPDATE "user"
                SET last_login_at = NOW()
                FROM user_key
                WHERE "user".id = $1 AND user_key.user_id = "user".id AND user_key.fingerprint = $2
                "#,
                user_id as _,
                fingerprint
            )
            .execute(tx)
            .await?
            .rows_affected()
                == 1
            {
                Ok(user_id)
            } else {
                Err(UserManagementError::InvalidAuth.into())
            }
        })
    }
    async fn create_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserId, UserManagementError> {
        transact!(UserManagementError, self.aero, |tx| {
            Ok(sqlx::query_scalar!(
                r#"
                WITH params AS (
                    SELECT gen_salt('bf') AS password_salt
                )
                INSERT INTO "user" (
                    username,
                    password_salt,
                    password_hash
                )
                SELECT
                    $1,
                    password_salt,
                    crypt($2, password_salt)
                FROM params
                ON CONFLICT DO NOTHING
                RETURNING id AS "id: _"
                "#,
                username,
                password
            )
            .fetch_optional(tx)
            .await?
            .ok_or(UserManagementError::UserAlreadyExists)?)
        })
    }
    async fn add_user_public_key(
        &self,
        user_id: UserId,
        fingerprint: &str,
    ) -> Result<(), UserManagementError> {
        transact!(UserManagementError, self.aero, |tx| {
            sqlx::query!(
                r#"
                    INSERT INTO user_key (user_id, fingerprint)
                    VALUES ($1, $2)
                    ON CONFLICT DO NOTHING
                    "#,
                user_id as _,
                fingerprint
            )
            .execute(tx)
            .await?;

            Ok(())
        })
    }
    async fn connect_terminal(
        &self,
        user_id: UserId,
    ) -> Result<TerminalConnection, UserManagementError> {
        Ok(TerminalSession::spawn(self.aero.clone(), user_id))
    }
}
