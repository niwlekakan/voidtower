use anyhow::{Context, Result};
use sqlx::SqlitePool;

pub(crate) async fn run(pool: &SqlitePool) -> Result<()> {
    let mut transaction = pool.begin().await?;
    seed_default_policy_rules(&mut transaction).await?;
    crate::voidwatch::allowlist_seed::seed_default_allowlist_if_empty_on(&mut transaction)
        .await
        .context("failed to seed the Voidwatch default allowlist")?;
    seed_operation_resources(&mut transaction)
        .await
        .context("failed to backfill operation resources")?;
    transaction.commit().await?;
    Ok(())
}

async fn seed_operation_resources(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<()> {
    for (kind, display_name, namespace, scope_key, alias) in [
        (
            "system",
            "This VoidTower",
            "voidtower.singleton",
            "local",
            "system",
        ),
        (
            "docker_engine",
            "Local Docker",
            "voidtower.singleton",
            "local",
            "docker",
        ),
        (
            "firewall",
            "Local Firewall",
            "voidtower.singleton",
            "local",
            "firewall",
        ),
        (
            "reverse_proxy_service",
            "Local Reverse Proxy",
            "voidtower.singleton",
            "local",
            "reverse-proxy",
        ),
        (
            "update_target",
            "VoidTower Update Target",
            "voidtower.update_target",
            "local",
            "voidtower",
        ),
        (
            "update_target",
            "Odysseus Update Target",
            "voidtower.update_target",
            "local",
            "odysseus",
        ),
        (
            "update_target",
            "Operating System Update Target",
            "voidtower.update_target",
            "local",
            "os",
        ),
    ] {
        ensure_resource(
            transaction,
            kind,
            display_name,
            None,
            Some("local"),
            namespace,
            scope_key,
            alias,
        )
        .await?;
    }

    let backup_configs: Vec<(String, String)> =
        sqlx::query_as("SELECT id, name FROM backup_configs")
            .fetch_all(&mut **transaction)
            .await?;
    for (id, name) in backup_configs {
        ensure_resource(
            transaction,
            "backup_config",
            &name,
            None,
            Some("restic"),
            "voidtower.backup_config",
            "local",
            &id,
        )
        .await?;
    }

    let proxy_configs: Vec<(String, String)> =
        sqlx::query_as("SELECT id, domain FROM proxy_configs")
            .fetch_all(&mut **transaction)
            .await?;
    for (id, domain) in proxy_configs {
        ensure_resource(
            transaction,
            "proxy_rule",
            &domain,
            None,
            Some("nginx"),
            "voidtower.proxy_config",
            "local",
            &id,
        )
        .await?;
    }

    let proxmox_hosts: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM proxmox_hosts")
        .fetch_all(&mut **transaction)
        .await?;
    for (id, name) in proxmox_hosts {
        ensure_resource(
            transaction,
            "proxmox_host",
            &name,
            None,
            Some("proxmox"),
            "voidtower.proxmox_host",
            "local",
            &id,
        )
        .await?;
    }

    for (sql, namespace) in [
        ("SELECT id, display_name FROM nodes", "voidtower.node"),
        (
            "SELECT id, name FROM node_registry",
            "voidtower.legacy_node",
        ),
    ] {
        let rows: Vec<(String, String)> = sqlx::query_as(sql).fetch_all(&mut **transaction).await?;
        for (id, name) in rows {
            ensure_resource(
                transaction,
                "managed_node",
                &name,
                Some(&id),
                Some("voidtower"),
                namespace,
                "local",
                &id,
            )
            .await?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn ensure_resource(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    kind: &str,
    display_name: &str,
    node_id: Option<&str>,
    provider: Option<&str>,
    namespace: &str,
    scope_key: &str,
    alias: &str,
) -> Result<String> {
    let now = unix_now();
    if let Some(resource_id) = sqlx::query_scalar::<_, String>(
        "SELECT resource_id FROM resource_aliases \
         WHERE namespace = ? AND scope_key = ? AND value = ?",
    )
    .bind(namespace)
    .bind(scope_key)
    .bind(alias)
    .fetch_optional(&mut **transaction)
    .await?
    {
        sqlx::query(
            "UPDATE resources SET display_name = ?, node_id = ?, provider = ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(display_name)
        .bind(node_id)
        .bind(provider)
        .bind(now)
        .bind(&resource_id)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            "UPDATE resource_aliases SET last_seen_at = ? \
             WHERE resource_id = ? AND namespace = ? AND scope_key = ? AND value = ?",
        )
        .bind(now)
        .bind(&resource_id)
        .bind(namespace)
        .bind(scope_key)
        .bind(alias)
        .execute(&mut **transaction)
        .await?;
        return Ok(resource_id);
    }

    let resource_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO resources \
         (id, kind, display_name, node_id, provider, lifecycle_state, revision, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, 'active', 0, ?, ?)",
    )
    .bind(&resource_id)
    .bind(kind)
    .bind(display_name)
    .bind(node_id)
    .bind(provider)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO resource_aliases \
         (resource_id, namespace, scope_key, value, created_at, last_seen_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&resource_id)
    .bind(namespace)
    .bind(scope_key)
    .bind(alias)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(resource_id)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
