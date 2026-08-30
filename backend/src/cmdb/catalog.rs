use anyhow::Result;
use sqlx::{Sqlite, Transaction};

const CLASSES: &[(&str, &str, &str)] = &[
    ("hw", "Hardware", "Hardware and physical components"),
    ("net", "Network", "Network infrastructure"),
    ("sys", "Systems", "Physical and logical computer systems"),
    ("svc", "Services", "Applications, services, and databases"),
    ("data", "Data", "Logical data and storage resources"),
    ("dev", "Development", "Development resources"),
    ("sec", "Security", "Security-related assets"),
    ("iot", "IoT", "Embedded and smart-home devices"),
    ("pwr", "Power", "Power infrastructure"),
    ("per", "Peripherals", "Computer peripherals"),
    ("loc", "Locations", "Physical inventory locations"),
    ("lic", "Licenses", "Licenses and subscriptions"),
];

const TYPES: &[(&str, &str, &str)] = &[
    ("hdd", "hw", "Hard disk drive"),
    ("ssd", "hw", "Solid-state drive"),
    ("ssd25", "hw", "2.5-inch solid-state drive"),
    ("ssdms", "hw", "mSATA solid-state drive"),
    ("ssdm2", "hw", "M.2 solid-state drive"),
    ("ssdpcie", "hw", "PCIe solid-state drive"),
    ("ssdu2", "hw", "U.2 solid-state drive"),
    ("ssdu3", "hw", "U.3 solid-state drive"),
    ("usb", "hw", "USB storage device"),
    ("sd", "hw", "SD card"),
    ("msd", "hw", "microSD card"),
    ("opt", "hw", "Optical drive"),
    ("tape", "hw", "Tape drive or medium"),
    ("host", "sys", "Managed host"),
    ("vm", "sys", "Virtual machine"),
    ("ct", "sys", "System container"),
    ("app", "svc", "Application or service"),
    ("db", "svc", "Database"),
    ("pool", "data", "Storage pool"),
    ("bkp", "data", "Backup resource"),
];

const RELATIONSHIP_TYPES: &[(&str, &str, &str)] = &[
    ("contains", "Contains", "Contained by"),
    ("installed_in", "Installed in", "Contains installation"),
    ("connected_to", "Connected to", "Connected to"),
    ("attached_to", "Attached to", "Has attachment"),
    ("hosts", "Hosts", "Hosted on"),
    ("hosted_on", "Hosted on", "Hosts"),
    ("runs", "Runs", "Runs on"),
    ("runs_on", "Runs on", "Runs"),
    ("member_of", "Member of", "Has member"),
    ("depends_on", "Depends on", "Dependency of"),
    ("stores", "Stores", "Stored on"),
    ("stored_on", "Stored on", "Stores"),
    ("backs_up", "Backs up", "Backed up by"),
    ("backed_up_by", "Backed up by", "Backs up"),
    ("powers", "Powers", "Powered by"),
    ("powered_by", "Powered by", "Powers"),
    ("located_at", "Located at", "Contains asset"),
    ("assigned_to", "Assigned to", "Has assignment"),
    ("managed_by", "Managed by", "Manages"),
];

pub async fn seed(transaction: &mut Transaction<'_, Sqlite>, now: i64) -> Result<()> {
    for (key, label, description) in CLASSES {
        sqlx::query(
            "INSERT INTO cmdb_classes \
             (key, label, description, is_builtin, enabled, created_at, updated_at) \
             VALUES (?, ?, ?, 1, 1, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET is_builtin = 1",
        )
        .bind(key)
        .bind(label)
        .bind(description)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }

    for (key, class_key, label) in TYPES {
        sqlx::query(
            "INSERT INTO cmdb_types \
             (key, class_key, label, is_builtin, enabled, created_at, updated_at) \
             VALUES (?, ?, ?, 1, 1, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET is_builtin = 1",
        )
        .bind(key)
        .bind(class_key)
        .bind(label)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }

    for (key, label, inverse_label) in RELATIONSHIP_TYPES {
        sqlx::query(
            "INSERT INTO cmdb_relationship_types \
             (key, label, inverse_label, is_builtin, enabled, created_at, updated_at) \
             VALUES (?, ?, ?, 1, 1, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET is_builtin = 1",
        )
        .bind(key)
        .bind(label)
        .bind(inverse_label)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }

    sqlx::query(
        "INSERT INTO cmdb_identifier_settings \
         (id, prefix, template, separator, number_width, starting_number, counter_scope, \
          letter_case, discovery_policy, updated_at) \
         VALUES ('default', 'VT', \
                 '{prefix}{separator}{class}{separator}{type}{separator}{number}', \
                 '-', 4, 1, 'class_type', 'preserve', 'trusted_providers', ?) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(now)
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn builtins_and_defaults_are_complete_and_idempotent() {
        let pool = pool().await;
        for now in [1, 2] {
            let mut transaction = pool.begin().await.unwrap();
            seed(&mut transaction, now).await.unwrap();
            transaction.commit().await.unwrap();
        }

        let class_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_classes")
            .fetch_one(&pool)
            .await
            .unwrap();
        let type_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_types")
            .fetch_one(&pool)
            .await
            .unwrap();
        let relationship_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_relationship_types")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(class_count, CLASSES.len() as i64);
        assert_eq!(type_count, TYPES.len() as i64);
        assert_eq!(relationship_count, RELATIONSHIP_TYPES.len() as i64);

        let settings: (String, String, i64, String, String) = sqlx::query_as(
            "SELECT prefix, separator, number_width, counter_scope, discovery_policy \
             FROM cmdb_identifier_settings WHERE id = 'default'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            settings,
            (
                "VT".into(),
                "-".into(),
                4,
                "class_type".into(),
                "trusted_providers".into()
            )
        );
    }
}
