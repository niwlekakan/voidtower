use crate::cmdb::contracts::IdentityEvidenceV1;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};
use std::collections::{BTreeMap, BTreeSet};

const MAX_IDENTITIES: usize = 32;
const MAX_IDENTITY_KIND_LEN: usize = 32;
const MAX_IDENTITY_VALUE_LEN: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum CorrelationError {
    #[error("invalid identity evidence: {0}")]
    Invalid(String),
    #[error("identity correlation failed")]
    Internal(#[source] sqlx::Error),
}

impl From<sqlx::Error> for CorrelationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error)
    }
}

pub type Result<T> = std::result::Result<T, CorrelationError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedIdentity {
    pub kind: String,
    pub value: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedEvidence {
    pub identities: Vec<NormalizedIdentity>,
    pub malformed_strong_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewReason {
    WeakIdentity,
    MalformedIdentity,
    AmbiguousIdentity,
    ContradictoryIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorrelationOutcome {
    Matched { resource_id: String },
    UnmatchedStrong,
    Review { reason: ReviewReason },
}

fn is_matchable_physical_identity(identity: &NormalizedIdentity) -> bool {
    matches!(
        identity.kind.as_str(),
        "wwn" | "nvme_uuid" | "nvme_eui" | "serial_model"
    )
}

fn collapse_whitespace(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn normalize_hex(value: &str, lengths: &[usize]) -> Option<String> {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let normalized: String = value
        .chars()
        .filter(|character| !matches!(character, ':' | '-' | '_' | ' ' | '.'))
        .flat_map(char::to_lowercase)
        .collect();
    if lengths.contains(&normalized.len())
        && normalized.bytes().all(|byte| byte.is_ascii_hexdigit())
        && normalized.bytes().any(|byte| byte != b'0')
    {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_uuid(value: &str) -> Option<String> {
    let parsed = uuid::Uuid::parse_str(value.trim()).ok()?;
    (!parsed.is_nil()).then(|| parsed.hyphenated().to_string())
}

fn bounded_value(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= MAX_IDENTITY_VALUE_LEN).then_some(value)
}

pub fn normalize(provider: &str, evidence: &[IdentityEvidenceV1]) -> Result<NormalizedEvidence> {
    if evidence.len() > MAX_IDENTITIES {
        return Err(CorrelationError::Invalid(format!(
            "identity count exceeds {MAX_IDENTITIES}"
        )));
    }
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty()
        || provider.len() > MAX_IDENTITY_KIND_LEN
        || !provider.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        })
    {
        return Err(CorrelationError::Invalid(
            "provider namespace is invalid".into(),
        ));
    }

    let mut normalized = BTreeMap::<(String, String), String>::new();
    let mut serial = None;
    let mut model = None;
    let mut malformed_strong_identity = false;
    for item in evidence {
        let kind = item.kind.trim().to_ascii_lowercase().replace('-', "_");
        if kind.is_empty() || kind.len() > MAX_IDENTITY_KIND_LEN {
            return Err(CorrelationError::Invalid("identity kind is invalid".into()));
        }
        let value = bounded_value(&item.value);
        let parsed = match kind.as_str() {
            "wwn" => value.and_then(|value| normalize_hex(value, &[16, 32])),
            "nvme_uuid" | "hardware_uuid" => value.and_then(normalize_uuid),
            "nvme_eui" => value.and_then(|value| normalize_hex(value, &[16, 32])),
            "serial" => value.map(collapse_whitespace),
            "model" => value.map(collapse_whitespace),
            "provider_id" => {
                value.map(|value| format!("{provider}:{}", collapse_whitespace(value)))
            }
            // Runtime paths are intentionally excluded from identity.
            "path" | "device_path" | "disk_number" | "drive_letter" => None,
            _ => None,
        };
        match (kind.as_str(), parsed) {
            ("wwn" | "nvme_uuid" | "nvme_eui" | "hardware_uuid", None) => {
                malformed_strong_identity = true;
            }
            ("serial", Some(value)) => serial = Some(value),
            ("model", Some(value)) => model = Some(value),
            ("provider_id", Some(value)) => {
                normalized.insert(("provider_id".into(), value), "weak".into());
            }
            ("wwn" | "nvme_uuid" | "nvme_eui" | "hardware_uuid", Some(value)) => {
                normalized.insert((kind, value), "strong".into());
            }
            _ => {}
        }
    }
    if let Some(serial) = serial.as_ref() {
        normalized.insert(("serial".into(), serial.clone()), "weak".into());
        if let Some(model) = model.as_ref() {
            let composite = serde_json::to_string(&(model, serial)).map_err(|error| {
                CorrelationError::Invalid(format!("serial/model identity is invalid: {error}"))
            })?;
            normalized.insert(("serial_model".into(), composite), "strong".into());
        }
    }

    Ok(NormalizedEvidence {
        identities: normalized
            .into_iter()
            .map(|((kind, value), confidence)| NormalizedIdentity {
                kind,
                value,
                confidence,
            })
            .collect(),
        malformed_strong_identity,
    })
}

async fn matches_for(
    transaction: &mut Transaction<'_, Sqlite>,
    identities: impl Iterator<Item = &NormalizedIdentity>,
) -> Result<BTreeSet<String>> {
    let mut matches = BTreeSet::new();
    for identity in identities {
        let resources: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT resource_id FROM cmdb_asset_identities \
             WHERE identity_kind = ? AND normalized_value = ? ORDER BY resource_id LIMIT 2",
        )
        .bind(&identity.kind)
        .bind(&identity.value)
        .fetch_all(&mut **transaction)
        .await?;
        matches.extend(resources);
    }
    Ok(matches)
}

pub async fn correlate(
    transaction: &mut Transaction<'_, Sqlite>,
    evidence: &NormalizedEvidence,
) -> Result<CorrelationOutcome> {
    if evidence.malformed_strong_identity {
        return Ok(CorrelationOutcome::Review {
            reason: ReviewReason::MalformedIdentity,
        });
    }

    let all_strong_matches = matches_for(
        transaction,
        evidence
            .identities
            .iter()
            .filter(|identity| is_matchable_physical_identity(identity)),
    )
    .await?;
    if all_strong_matches.len() > 1 {
        return Ok(CorrelationOutcome::Review {
            reason: ReviewReason::ContradictoryIdentity,
        });
    }

    for kinds in [
        &["wwn"][..],
        &["nvme_uuid", "nvme_eui"][..],
        &["serial_model"][..],
    ] {
        let matches = matches_for(
            transaction,
            evidence
                .identities
                .iter()
                .filter(|identity| kinds.contains(&identity.kind.as_str())),
        )
        .await?;
        match matches.len() {
            0 => {}
            1 => {
                return Ok(CorrelationOutcome::Matched {
                    resource_id: matches.into_iter().next().unwrap_or_default(),
                });
            }
            _ => {
                return Ok(CorrelationOutcome::Review {
                    reason: ReviewReason::AmbiguousIdentity,
                });
            }
        }
    }

    if evidence
        .identities
        .iter()
        .any(is_matchable_physical_identity)
    {
        Ok(CorrelationOutcome::UnmatchedStrong)
    } else {
        Ok(CorrelationOutcome::Review {
            reason: ReviewReason::WeakIdentity,
        })
    }
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

    fn evidence(items: &[(&str, &str)]) -> Vec<IdentityEvidenceV1> {
        items
            .iter()
            .map(|(kind, value)| IdentityEvidenceV1 {
                kind: (*kind).into(),
                value: (*value).into(),
            })
            .collect()
    }

    async fn identity(pool: &SqlitePool, resource_id: &str, kind: &str, value: &str) {
        let now = 1;
        sqlx::query(
            "INSERT INTO resources \
             (id, kind, display_name, lifecycle_state, revision, created_at, updated_at) \
             VALUES (?, 'cmdb_asset', ?, 'active', 0, ?, ?)",
        )
        .bind(resource_id)
        .bind(resource_id)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cmdb_asset_identities \
             (id, resource_id, identity_kind, normalized_value, confidence, source, \
              first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, 'strong', 'test', ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(resource_id)
        .bind(kind)
        .bind(value)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn normalization_is_path_independent_and_derives_serial_model() {
        let normalized = normalize(
            "agent",
            &evidence(&[
                ("path", "/dev/sdc"),
                ("wwn", "0x50:00-C5-00_ABCD.1234"),
                ("model", "  Samsung   SSD  "),
                ("serial", " ABC 123 "),
            ]),
        )
        .unwrap();
        assert!(!normalized
            .identities
            .iter()
            .any(|identity| identity.kind == "path"));
        assert!(normalized
            .identities
            .iter()
            .any(|identity| { identity.kind == "wwn" && identity.value == "5000c500abcd1234" }));
        assert!(normalized.identities.iter().any(|identity| {
            identity.kind == "serial_model" && identity.value == r#"["samsung ssd","abc 123"]"#
        }));
    }

    #[tokio::test]
    async fn matching_uses_priority_and_rejects_conflicting_strong_evidence() {
        let pool = pool().await;
        identity(&pool, "asset-a", "wwn", "5000c500abcd1234").await;
        identity(
            &pool,
            "asset-b",
            "nvme_uuid",
            "98245011-aefa-4b0c-a507-c579a2639a32",
        )
        .await;
        let mut transaction = pool.begin().await.unwrap();
        let matched = normalize("agent", &evidence(&[("wwn", "50:00:c5:00:ab:cd:12:34")])).unwrap();
        assert_eq!(
            correlate(&mut transaction, &matched).await.unwrap(),
            CorrelationOutcome::Matched {
                resource_id: "asset-a".into()
            }
        );
        let conflict = normalize(
            "agent",
            &evidence(&[
                ("wwn", "50:00:c5:00:ab:cd:12:34"),
                ("nvme_uuid", "98245011-aefa-4b0c-a507-c579a2639a32"),
            ]),
        )
        .unwrap();
        assert_eq!(
            correlate(&mut transaction, &conflict).await.unwrap(),
            CorrelationOutcome::Review {
                reason: ReviewReason::ContradictoryIdentity
            }
        );
    }

    #[tokio::test]
    async fn weak_malformed_and_unmatched_strong_evidence_have_distinct_outcomes() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let weak = normalize("agent", &evidence(&[("serial", "same everywhere")])).unwrap();
        assert_eq!(
            correlate(&mut transaction, &weak).await.unwrap(),
            CorrelationOutcome::Review {
                reason: ReviewReason::WeakIdentity
            }
        );
        let malformed = normalize("agent", &evidence(&[("wwn", "not-a-wwn")])).unwrap();
        assert_eq!(
            correlate(&mut transaction, &malformed).await.unwrap(),
            CorrelationOutcome::Review {
                reason: ReviewReason::MalformedIdentity
            }
        );
        let strong = normalize("agent", &evidence(&[("wwn", "50:00:c5:00:ab:cd:12:34")])).unwrap();
        assert_eq!(
            correlate(&mut transaction, &strong).await.unwrap(),
            CorrelationOutcome::UnmatchedStrong
        );
    }
}
