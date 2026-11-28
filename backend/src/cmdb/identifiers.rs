use super::contracts::IdentifierSettings;
use anyhow::{bail, Context, Result};
use sqlx::{Sqlite, Transaction};

const MAX_KEY_LEN: usize = 32;
const MAX_PREFIX_LEN: usize = 32;
const MAX_SEPARATOR_LEN: usize = 4;
const MAX_TEMPLATE_LEN: usize = 128;
const MAX_ASSET_ID_LEN: usize = 128;
const ALLOWED_TOKENS: &[&str] = &["prefix", "separator", "class", "type", "number"];

pub fn validate_key(value: &str, field: &str) -> Result<()> {
    if value.is_empty() || value.len() > MAX_KEY_LEN {
        bail!("{field} must be between 1 and {MAX_KEY_LEN} characters");
    }
    if !value.bytes().enumerate().all(|(index, byte)| match byte {
        b'a'..=b'z' | b'0'..=b'9' => true,
        b'_' | b'-' => index > 0,
        _ => false,
    }) {
        bail!("{field} must use lowercase ASCII letters, numbers, underscores, or hyphens");
    }
    Ok(())
}

pub fn validate_settings(settings: &IdentifierSettings) -> Result<()> {
    if settings.prefix.is_empty() || settings.prefix.len() > MAX_PREFIX_LEN {
        bail!("prefix must be between 1 and {MAX_PREFIX_LEN} characters");
    }
    if !settings
        .prefix
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric())
    {
        bail!("prefix must contain only ASCII letters and numbers");
    }
    if settings.separator.is_empty() || settings.separator.len() > MAX_SEPARATOR_LEN {
        bail!("separator must be between 1 and {MAX_SEPARATOR_LEN} characters");
    }
    if !settings
        .separator
        .bytes()
        .all(|byte| matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("separator must contain only hyphens, underscores, or dots");
    }
    if settings.template.is_empty() || settings.template.len() > MAX_TEMPLATE_LEN {
        bail!("template must be between 1 and {MAX_TEMPLATE_LEN} characters");
    }
    validate_template(&settings.template)?;
    if !(1..=12).contains(&settings.number_width) {
        bail!("number_width must be between 1 and 12");
    }
    if settings.starting_number <= 0 {
        bail!("starting_number must be greater than zero");
    }
    if !matches!(
        settings.counter_scope.as_str(),
        "global" | "class" | "type" | "class_type"
    ) {
        bail!("counter_scope is invalid");
    }
    if !matches!(
        settings.letter_case.as_str(),
        "preserve" | "lower" | "upper"
    ) {
        bail!("letter_case is invalid");
    }
    if !matches!(
        settings.discovery_policy.as_str(),
        "off" | "review_first" | "trusted_providers" | "automatic"
    ) {
        bail!("discovery_policy is invalid");
    }
    Ok(())
}

fn validate_template(template: &str) -> Result<()> {
    let mut remainder = template;
    let mut has_number = false;
    while let Some(open) = remainder.find('{') {
        if remainder[..open].contains('}') {
            bail!("template contains an unmatched closing brace");
        }
        let after_open = &remainder[open + 1..];
        let Some(close) = after_open.find('}') else {
            bail!("template contains an unmatched opening brace");
        };
        let token = &after_open[..close];
        if !ALLOWED_TOKENS.contains(&token) {
            bail!("template contains unknown field {{{token}}}");
        }
        has_number |= token == "number";
        remainder = &after_open[close + 1..];
    }
    if remainder.contains('}') {
        bail!("template contains an unmatched closing brace");
    }
    if !has_number {
        bail!("template must contain {{number}}");
    }
    Ok(())
}

pub fn render(
    settings: &IdentifierSettings,
    class_key: &str,
    type_key: &str,
    number: i64,
) -> Result<String> {
    validate_settings(settings)?;
    validate_key(class_key, "class")?;
    validate_key(type_key, "type")?;
    if number <= 0 {
        bail!("identifier number must be greater than zero");
    }
    let width = usize::try_from(settings.number_width).context("invalid number width")?;
    let number = format!("{number:0width$}");
    let mut rendered = settings
        .template
        .replace("{prefix}", &settings.prefix)
        .replace("{separator}", &settings.separator)
        .replace("{class}", class_key)
        .replace("{type}", type_key)
        .replace("{number}", &number);
    rendered = match settings.letter_case.as_str() {
        "lower" => rendered.to_ascii_lowercase(),
        "upper" => rendered.to_ascii_uppercase(),
        _ => rendered,
    };
    if rendered.is_empty() || rendered.len() > MAX_ASSET_ID_LEN {
        bail!("rendered asset identifier exceeds its allowed length");
    }
    if !rendered
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("rendered asset identifier contains unsupported characters");
    }
    Ok(rendered)
}

pub async fn load_settings(
    transaction: &mut Transaction<'_, Sqlite>,
) -> Result<IdentifierSettings> {
    let settings = sqlx::query_as(
        "SELECT prefix, template, separator, number_width, starting_number, counter_scope, \
                letter_case, discovery_policy, updated_at \
         FROM cmdb_identifier_settings WHERE id = 'default'",
    )
    .fetch_one(&mut **transaction)
    .await
    .context("CMDB identifier settings are missing")?;
    validate_settings(&settings)?;
    Ok(settings)
}

pub async fn allocate(
    transaction: &mut Transaction<'_, Sqlite>,
    class_key: &str,
    type_key: &str,
    now: i64,
) -> Result<String> {
    validate_key(class_key, "class")?;
    validate_key(type_key, "type")?;
    let type_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cmdb_types \
         WHERE key = ? AND class_key = ? AND enabled = 1 \
           AND EXISTS (SELECT 1 FROM cmdb_classes WHERE key = ? AND enabled = 1)",
    )
    .bind(type_key)
    .bind(class_key)
    .bind(class_key)
    .fetch_one(&mut **transaction)
    .await?;
    if type_exists != 1 {
        bail!("CMDB class/type pair is unavailable");
    }

    let settings = load_settings(transaction).await?;
    let scope_key = match settings.counter_scope.as_str() {
        "global" => "global".to_string(),
        "class" => format!("class:{class_key}"),
        "type" => format!("type:{type_key}"),
        _ => format!("class_type:{class_key}:{type_key}"),
    };
    let next_after_insert = settings
        .starting_number
        .checked_add(1)
        .context("identifier counter overflow")?;
    let number: i64 = sqlx::query_scalar(
        "INSERT INTO cmdb_identifier_counters (scope_key, next_number, updated_at) \
         VALUES (?, ?, ?) \
         ON CONFLICT(scope_key) DO UPDATE SET \
             next_number = cmdb_identifier_counters.next_number + 1, \
             updated_at = excluded.updated_at \
         RETURNING next_number - 1",
    )
    .bind(scope_key)
    .bind(next_after_insert)
    .bind(now)
    .fetch_one(&mut **transaction)
    .await?;
    render(&settings, class_key, type_key, number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    fn settings() -> IdentifierSettings {
        IdentifierSettings {
            prefix: "VT".into(),
            template: "{prefix}{separator}{class}{separator}{type}{separator}{number}".into(),
            separator: "-".into(),
            number_width: 4,
            starting_number: 1,
            counter_scope: "class_type".into(),
            letter_case: "preserve".into(),
            discovery_policy: "trusted_providers".into(),
            updated_at: 0,
        }
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let mut transaction = pool.begin().await.unwrap();
        crate::cmdb::catalog::seed(&mut transaction, 1)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        pool
    }

    #[test]
    fn default_and_custom_formats_are_stable() {
        assert_eq!(
            render(&settings(), "hw", "hdd", 1).unwrap(),
            "VT-hw-hdd-0001"
        );
        let custom = IdentifierSettings {
            prefix: "LAB".into(),
            template: "{prefix}{separator}{class}{separator}{type}{separator}{number}".into(),
            separator: ".".into(),
            number_width: 5,
            letter_case: "upper".into(),
            ..settings()
        };
        assert_eq!(
            render(&custom, "hw", "ssdm2", 7).unwrap(),
            "LAB.HW.SSDM2.00007"
        );
    }

    #[test]
    fn invalid_templates_and_unsafe_output_fail_closed() {
        for template in [
            "{prefix}-{unknown}-{number}",
            "{prefix}-{type}",
            "{number",
            "number}",
        ] {
            let invalid = IdentifierSettings {
                template: template.into(),
                ..settings()
            };
            assert!(validate_settings(&invalid).is_err(), "accepted {template}");
        }
        let invalid = IdentifierSettings {
            template: "asset/{number}".into(),
            ..settings()
        };
        assert!(render(&invalid, "hw", "hdd", 1).is_err());
        assert!(validate_key("Bad Key", "class").is_err());
    }

    #[tokio::test]
    async fn allocation_is_sequential_and_scoped_by_class_and_type() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        assert_eq!(
            allocate(&mut transaction, "hw", "hdd", 1).await.unwrap(),
            "VT-hw-hdd-0001"
        );
        assert_eq!(
            allocate(&mut transaction, "hw", "hdd", 1).await.unwrap(),
            "VT-hw-hdd-0002"
        );
        assert_eq!(
            allocate(&mut transaction, "hw", "ssdm2", 1).await.unwrap(),
            "VT-hw-ssdm2-0001"
        );
        transaction.commit().await.unwrap();
    }

    #[tokio::test]
    async fn settings_changes_do_not_rewind_existing_counters() {
        let pool = pool().await;
        let mut first = pool.begin().await.unwrap();
        assert_eq!(
            allocate(&mut first, "hw", "hdd", 1).await.unwrap(),
            "VT-hw-hdd-0001"
        );
        first.commit().await.unwrap();
        sqlx::query(
            "UPDATE cmdb_identifier_settings SET prefix = 'HOME', number_width = 6, updated_at = 2 \
             WHERE id = 'default'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut second = pool.begin().await.unwrap();
        assert_eq!(
            allocate(&mut second, "hw", "hdd", 2).await.unwrap(),
            "HOME-hw-hdd-000002"
        );
        second.commit().await.unwrap();
    }
}
