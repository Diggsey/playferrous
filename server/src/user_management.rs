use async_trait::async_trait;
use playferrous_presentation::{
    TerminalConnection, TerminalServerCommand, UserId, UserManagement, UserManagementError,
};
use sqlx::{Postgres, Transaction};
use tokio::sync::mpsc;

use crate::database::{Database, TransactError};

pub struct UserManagementImpl {
    database: Database,
}

impl UserManagementImpl {
    pub fn new(database: Database) -> Self {
        Self { database }
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
        Ok(UserId(
            sqlx::query_scalar!(
                r#"
                SELECT id FROM "user"
                WHERE username = $1
                "#,
                username
            )
            .fetch_optional(tx)
            .await?
            .ok_or(UserManagementError::InvalidAuth)?,
        ))
    }
}

#[async_trait]
impl UserManagement for UserManagementImpl {
    async fn login_user_with_password(
        &self,
        username: &str,
        password: &str,
    ) -> Result<UserId, UserManagementError> {
        transact!(self.database, |tx| {
            let user_id = self.find_user_id(tx, username).await?;
            if sqlx::query!(
                r#"
                UPDATE "user"
                SET last_login_at = NOW()
                WHERE id = $1 AND password_hash = crypt($2, password_salt)
                "#,
                user_id.0,
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
        transact!(self.database, |tx| {
            let user_id = self.find_user_id(tx, username).await?;
            if sqlx::query!(
                r#"
                UPDATE "user"
                SET last_login_at = NOW()
                FROM user_key
                WHERE "user".id = $1 AND user_key.user_id = "user".id AND user_key.fingerprint = $2
                "#,
                user_id.0,
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
        transact!(self.database, |tx| {
            Ok(UserId(
                sqlx::query_scalar!(
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
                    RETURNING id
                    "#,
                    username,
                    password
                )
                .fetch_optional(tx)
                .await?
                .ok_or(UserManagementError::UserAlreadyExists)?,
            ))
        })
    }
    async fn add_user_public_key(
        &self,
        user_id: UserId,
        fingerprint: &str,
    ) -> Result<(), UserManagementError> {
        transact!(self.database, |tx| {
            sqlx::query!(
                r#"
                    INSERT INTO user_key (user_id, fingerprint)
                    VALUES ($1, $2)
                    ON CONFLICT DO NOTHING
                    "#,
                user_id.0,
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
        let (tx1, mut rx1) = mpsc::channel(32);
        let (tx2, rx2) = mpsc::channel(32);
        tokio::spawn(async move {
            while let Some(item) = rx1.recv().await {
                println!("{:?}", item)
            }
        });
        tokio::spawn(async move {
            while tx2
                .send(TerminalServerCommand::RequestLine {
                    prompt: format!("user {}> ", user_id.0),
                })
                .await
                .is_ok()
            {}
        });
        Ok(TerminalConnection {
            sender: tx1,
            receiver: rx2,
        })
    }
}
