use std::collections::{HashMap, HashSet};

use aerosol::Aero;
use chrono::{DateTime, Utc};
use playferrous_presentation::{GroupId, MessageId, MessageMin, RequestId, UserId, UserMin};

use crate::connection_manager::{ConnectionManager, SystemToConnectionMsg};

use super::transaction::Transaction;

#[derive(Debug)]
pub struct Message {
    pub id: RequestId,
    pub to_id: UserId,
    pub from_id: Option<UserId>,
    pub subject: String,
    pub body: String,
    pub was_read: bool,
    pub request_id: Option<RequestId>,
    pub sent_at: DateTime<Utc>,
}

pub async fn send_to_user(
    tx: &mut Transaction,
    to_id: UserId,
    from_id: Option<UserId>,
    subject: String,
    body: String,
    request_id: Option<RequestId>,
) -> sqlx::Result<Message> {
    let message = sqlx::query_as!(
        Message,
        r#"
        INSERT INTO message (
            to_id,
            from_id,
            subject,
            body,
            request_id
        ) VALUES (
            $1,
            $2,
            $3,
            $4,
            $5
        )
        RETURNING
            id as "id: _",
            to_id as "to_id: _",
            from_id as "from_id: _",
            subject,
            body,
            was_read,
            request_id as "request_id: _",
            sent_at
        "#,
        to_id as _,
        from_id as _,
        subject,
        body,
        request_id as _
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.on_commit(move |aero: Aero| async move {
        let conn_mgr = aero.obtain::<ConnectionManager>();
        conn_mgr
            .send(to_id, SystemToConnectionMsg::NewMessage)
            .await;
        Ok(())
    });

    Ok(message)
}

pub async fn send_to_group(
    tx: &mut Transaction,
    to_id: GroupId,
    from_id: Option<UserId>,
    subject: String,
    body: String,
    request_id: Option<RequestId>,
) -> sqlx::Result<Vec<Message>> {
    let messages = sqlx::query_as!(
        Message,
        r#"
        INSERT INTO message (
            to_id,
            from_id,
            subject,
            body,
            request_id
        ) SELECT
            member_id,
            $2,
            $3,
            $4,
            $5
        FROM group_member
        WHERE group_id = $1
        RETURNING
            id as "id: _",
            to_id as "to_id: _",
            from_id as "from_id: _",
            subject,
            body,
            was_read,
            request_id as "request_id: _",
            sent_at
        "#,
        to_id as _,
        from_id as _,
        subject,
        body,
        request_id as _
    )
    .fetch_all(&mut *tx)
    .await?;

    let user_ids: HashSet<_> = messages.iter().map(|m| m.to_id).collect();
    tx.on_commit(move |aero: Aero| async move {
        let conn_mgr = aero.obtain::<ConnectionManager>();
        conn_mgr
            .broadcast(user_ids, |_| SystemToConnectionMsg::NewMessage)
            .await;
        Ok(())
    });

    Ok(messages)
}

struct MessageMinRecord {
    pub id: MessageId,
    pub sent_at: DateTime<Utc>,
    pub subject: String,
    pub from_id: Option<UserId>,
    pub request_id: Option<RequestId>,
}

impl MessageMinRecord {
    pub fn reify(self, users: &HashMap<UserId, UserMin>) -> MessageMin {
        MessageMin {
            id: self.id,
            sent_at: self.sent_at,
            subject: self.subject,
            from: self
                .from_id
                .map(|user_id| users.get(&user_id).expect("User to exist").clone()),
            request_id: self.request_id,
        }
    }
}

pub async fn list_for_user(tx: &mut Transaction, user_id: UserId) -> sqlx::Result<Vec<MessageMin>> {
    let records = sqlx::query_as!(
        MessageMinRecord,
        r#"
        SELECT
            id as "id: _",
            sent_at,
            subject,
            from_id as "from_id: _",
            request_id as "request_id: _"
        FROM message
        WHERE to_id = $1 AND NOT was_read
        ORDER BY sent_at DESC
        "#,
        user_id as _
    )
    .fetch_all(&mut *tx)
    .await?;

    let user_ids = records.iter().flat_map(|r| r.from_id);
    let users = super::user::get_by_ids(tx, user_ids).await?;

    Ok(records.into_iter().map(|r| r.reify(&users)).collect())
}
