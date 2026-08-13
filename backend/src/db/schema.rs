use anyhow::{bail, Context, Result};
use sqlx::{Connection, Row, SqliteConnection};
use std::collections::BTreeMap;

pub(crate) const BASELINE_SQL: &str = include_str!("../../migrations/0001_current_baseline.sql");

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColumnShape {
    data_type: String,
    not_null: bool,
    default_value: Option<String>,
    primary_key_position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ForeignKeyShape {
    referenced_table: String,
    from_column: String,
    to_column: Option<String>,
    on_update: String,
    on_delete: String,
    match_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TableShape {
    columns: BTreeMap<String, ColumnShape>,
    foreign_keys: Vec<ForeignKeyShape>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexShape {
    sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SchemaShape {
    tables: BTreeMap<String, TableShape>,
    indexes: BTreeMap<String, IndexShape>,
}

fn quoted_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn normalize_type(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn normalize_default(value: Option<String>) -> Option<String> {
    value.map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn normalize_sql(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

async fn table_names(connection: &mut SqliteConnection) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' \
         ORDER BY name",
    )
    .fetch_all(connection)
    .await
    .context("failed to inspect SQLite table names")
}

async fn inspect_table(connection: &mut SqliteConnection, table: &str) -> Result<TableShape> {
    let pragma = format!("PRAGMA table_info({})", quoted_identifier(table));
    let rows = sqlx::query(&pragma)
        .fetch_all(&mut *connection)
        .await
        .with_context(|| format!("failed to inspect columns for managed table {table}"))?;

    let mut columns = BTreeMap::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        columns.insert(
            name,
            ColumnShape {
                data_type: normalize_type(row.try_get::<String, _>("type")?.as_str()),
                not_null: row.try_get::<i64, _>("notnull")? != 0,
                default_value: normalize_default(row.try_get("dflt_value")?),
                primary_key_position: row.try_get("pk")?,
            },
        );
    }

    let pragma = format!("PRAGMA foreign_key_list({})", quoted_identifier(table));
    let rows = sqlx::query(&pragma)
        .fetch_all(&mut *connection)
        .await
        .with_context(|| format!("failed to inspect foreign keys for managed table {table}"))?;
    let mut foreign_keys = Vec::with_capacity(rows.len());
    for row in rows {
        foreign_keys.push(ForeignKeyShape {
            referenced_table: row.try_get("table")?,
            from_column: row.try_get("from")?,
            to_column: row.try_get("to")?,
            on_update: row.try_get("on_update")?,
            on_delete: row.try_get("on_delete")?,
            match_kind: row.try_get("match")?,
        });
    }
    foreign_keys.sort();

    Ok(TableShape {
        columns,
        foreign_keys,
    })
}

async fn inspect_schema(connection: &mut SqliteConnection) -> Result<SchemaShape> {
    let mut tables = BTreeMap::new();
    for table in table_names(connection).await? {
        tables.insert(table.clone(), inspect_table(connection, &table).await?);
    }

    let rows = sqlx::query(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
    )
    .fetch_all(&mut *connection)
    .await
    .context("failed to inspect managed SQLite indexes")?;
    let mut indexes = BTreeMap::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        let sql: String = row.try_get("sql")?;
        indexes.insert(
            name,
            IndexShape {
                sql: normalize_sql(&sql),
            },
        );
    }

    Ok(SchemaShape { tables, indexes })
}

async fn canonical_schema() -> Result<SchemaShape> {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .context("failed to open canonical in-memory schema")?;
    sqlx::query(BASELINE_SQL)
        .execute(&mut connection)
        .await
        .context("failed to construct canonical schema from baseline migration")?;
    inspect_schema(&mut connection).await
}

pub(crate) async fn validate_connection(connection: &mut SqliteConnection) -> Result<()> {
    let expected = canonical_schema().await?;
    let actual = inspect_schema(connection).await?;

    for (table, expected_shape) in expected.tables {
        let Some(actual_shape) = actual.tables.get(&table) else {
            bail!("managed table {table} is missing after legacy normalization");
        };

        for (column, expected_column) in &expected_shape.columns {
            let Some(actual_column) = actual_shape.columns.get(column) else {
                bail!("managed column {table}.{column} is missing after legacy normalization");
            };
            if actual_column != expected_column {
                bail!(
                    "managed column {table}.{column} has an incompatible definition: \
                     expected {expected_column:?}, found {actual_column:?}"
                );
            }
        }

        for foreign_key in &expected_shape.foreign_keys {
            if !actual_shape.foreign_keys.contains(foreign_key) {
                bail!("managed table {table} is missing required foreign key {foreign_key:?}");
            }
        }
    }

    for (index, expected_shape) in expected.indexes {
        let Some(actual_shape) = actual.indexes.get(&index) else {
            bail!("managed index {index} is missing after legacy normalization");
        };
        if actual_shape != &expected_shape {
            bail!(
                "managed index {index} has an incompatible definition: expected {}, found {}",
                expected_shape.sql,
                actual_shape.sql
            );
        }
    }

    Ok(())
}

pub(crate) async fn validate_integrity(connection: &mut SqliteConnection) -> Result<()> {
    let quick_check: Vec<String> = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_all(&mut *connection)
        .await
        .context("failed to run SQLite quick_check")?;
    if quick_check.as_slice() != ["ok"] {
        bail!("SQLite quick_check failed: {}", quick_check.join("; "));
    }

    let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut *connection)
        .await
        .context("failed to run SQLite foreign_key_check")?;
    if !foreign_key_violations.is_empty() {
        let mut details = Vec::new();
        for row in foreign_key_violations.iter().take(5) {
            let table: String = row.try_get("table")?;
            let row_id: Option<i64> = row.try_get("rowid")?;
            let parent: String = row.try_get("parent")?;
            let foreign_key_id: i64 = row.try_get("fkid")?;
            details.push(format!(
                "table={table}, rowid={}, parent={parent}, foreign_key={foreign_key_id}",
                row_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ));
        }
        bail!(
            "SQLite foreign_key_check found {} violation(s): {}",
            foreign_key_violations.len(),
            details.join("; ")
        );
    }

    Ok(())
}
