use anyhow::{Context, Result};
use sqlx::SqlitePool;

pub(crate) async fn run(pool: &SqlitePool) -> Result<()> {
    let mut transaction = pool.begin().await?;
    seed_default_policy_rules(&mut transaction).await?;
    crate::voidwatch::allowlist_seed::seed_default_allowlist_if_empty_on(&mut transaction)
        .await
        .context("failed to seed the Voidwatch default allowlist")?;
    transaction.commit().await?;
    Ok(())
}

async fn seed_default_policy_rules(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<()> {
    let rule_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_rules")
        .fetch_one(&mut **transaction)
        .await?;
    if rule_count != 0 {
        return Ok(());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let seeds = [
        (
            "Block AI access to ai-no-touch resources",
            "api_token",
            "*",
            "*",
            Some("ai-no-touch"),
            "deny",
            10i64,
        ),
        (
            "Block AI access to critical resources",
            "api_token",
            "*",
            "*",
            Some("critical"),
            "deny",
            20i64,
        ),
        (
            "Block API tokens from deleting anything",
            "api_token",
            "remove",
            "*",
            None,
            "deny",
            30i64,
        ),
    ];

    for (name, actor_type, action, resource_type, resource_tag, effect, priority) in seeds {
        sqlx::query(
            "INSERT INTO policy_rules \
             (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(name)
        .bind(actor_type)
        .bind(action)
        .bind(resource_type)
        .bind(resource_tag)
        .bind(effect)
        .bind(priority)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }

    Ok(())
}
