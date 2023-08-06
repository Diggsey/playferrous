use std::collections::HashMap;

use playferrous_presentation::{UserId, UserMin};

use super::transaction::Transaction;

pub async fn get_by_ids(
    tx: &mut Transaction,
    user_ids: impl IntoIterator<Item = UserId>,
) -> sqlx::Result<HashMap<UserId, UserMin>> {
    let user_ids: Vec<UserId> = user_ids.into_iter().collect();
    Ok(sqlx::query_as!(
        UserMin,
        r#"
        SELECT
            id as "id: _",
            username
        FROM "user"
        WHERE id = ANY($1)
        "#,
        &user_ids as &[UserId]
    )
    .fetch_all(tx)
    .await?
    .into_iter()
    .map(|u| (u.id, u))
    .collect())
}
