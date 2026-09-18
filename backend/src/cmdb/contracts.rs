use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

const MAX_JSON_DEPTH: usize = 8;
const MAX_JSON_VALUES: usize = 256;
const MAX_JSON_COLLECTION_LEN: usize = 128;
const MAX_JSON_STRING_LEN: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLifecycle {
    Unknown,
    New,
    Inventory,
    Testing,
    Available,
    Reserved,
    Deployed,
    Maintenance,
    Degraded,
    Quarantine,
    WipePending,
    Wiping,
    Wiped,
    Retired,
    Disposed,
    Lost,
}

impl AssetLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::New => "new",
            Self::Inventory => "inventory",
            Self::Testing => "testing",
            Self::Available => "available",
            Self::Reserved => "reserved",
            Self::Deployed => "deployed",
            Self::Maintenance => "maintenance",
            Self::Degraded => "degraded",
            Self::Quarantine => "quarantine",
            Self::WipePending => "wipe_pending",
            Self::Wiping => "wiping",
            Self::Wiped => "wiped",
            Self::Retired => "retired",
            Self::Disposed => "disposed",
            Self::Lost => "lost",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryStatus {
    Online,
    Offline,
    Missing,
    Manual,
    Unmanaged,
    Ignored,
    Stale,
}

impl DiscoveryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Missing => "missing",
            Self::Manual => "manual",
            Self::Unmanaged => "unmanaged",
            Self::Ignored => "ignored",
            Self::Stale => "stale",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetCondition {
    Unknown,
    New,
    Good,
    Fair,
    Poor,
    Damaged,
    Failed,
}

impl AssetCondition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::New => "new",
            Self::Good => "good",
            Self::Fair => "fair",
            Self::Poor => "poor",
            Self::Damaged => "damaged",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterScope {
    Global,
    Class,
    Type,
    ClassType,
}

impl CounterScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Class => "class",
            Self::Type => "type",
            Self::ClassType => "class_type",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierCase {
    Preserve,
    Lower,
    Upper,
}

impl IdentifierCase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Lower => "lower",
            Self::Upper => "upper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryPolicy {
    Off,
    ReviewFirst,
    TrustedProviders,
    Automatic,
}

impl DiscoveryPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::ReviewFirst => "review_first",
            Self::TrustedProviders => "trusted_providers",
            Self::Automatic => "automatic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct IdentifierSettings {
    pub prefix: String,
    pub template: String,
    pub separator: String,
    pub number_width: i64,
    pub starting_number: i64,
    pub counter_scope: String,
    pub letter_case: String,
    pub discovery_policy: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AssetRecord {
    pub resource_id: String,
    pub resource_kind: String,
    pub display_name: String,
    pub asset_id: String,
    pub class_key: String,
    pub type_key: String,
    pub subtype: Option<String>,
    pub friendly_name: Option<String>,
    pub description: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub part_number: Option<String>,
    pub lifecycle_status: String,
    pub discovery_status: String,
    pub condition_status: String,
    pub location_id: Option<String>,
    pub first_seen_at: Option<i64>,
    pub last_seen_at: Option<i64>,
    pub metadata_json: String,
    pub notes: String,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityEvidenceV1 {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostObservationV1 {
    pub entity_key: String,
    #[serde(default)]
    pub identities: Vec<IdentityEvidenceV1>,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub runtime: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservedEntityV1 {
    pub entity_key: String,
    pub entity_type: String,
    #[serde(default)]
    pub identities: Vec<IdentityEvidenceV1>,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub runtime: Value,
    #[serde(default)]
    pub health: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventorySnapshotV1 {
    pub schema_version: u16,
    pub snapshot_id: String,
    pub collector_version: String,
    pub platform: String,
    pub collected_at: i64,
    pub host: HostObservationV1,
    #[serde(default)]
    pub entities: Vec<ObservedEntityV1>,
}

impl InventorySnapshotV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("only inventory schema version 1 is supported".into());
        }
        uuid::Uuid::parse_str(self.snapshot_id.trim())
            .map_err(|_| "snapshot_id must be a UUID".to_owned())?;
        validate_text(&self.collector_version, "collector_version", 64)?;
        validate_text(&self.platform, "platform", 32)?;
        if self.collected_at <= 0 {
            return Err("collected_at must be a positive Unix timestamp".into());
        }
        let host_key = validate_text(&self.host.entity_key, "host.entity_key", 256)?;
        validate_identities(&self.host.identities, "host.identities")?;
        validate_json_object(&self.host.attributes, "host.attributes")?;
        validate_json_object(&self.host.runtime, "host.runtime")?;
        if self.entities.len() > 512 {
            return Err("entity count exceeds 512".into());
        }
        let mut keys = HashSet::new();
        keys.insert(host_key);
        for entity in &self.entities {
            let key = validate_text(&entity.entity_key, "entity_key", 256)?;
            validate_text(&entity.entity_type, "entity_type", 64)?;
            if !keys.insert(key) {
                return Err("entity keys must be non-empty and unique within a snapshot".into());
            }
            validate_identities(&entity.identities, "entity.identities")?;
            validate_json_object(&entity.attributes, "entity.attributes")?;
            validate_json_object(&entity.runtime, "entity.runtime")?;
            validate_json_object(&entity.health, "entity.health")?;
        }
        Ok(())
    }
}

fn validate_text(value: &str, field: &str, maximum: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field} is required"));
    }
    if value.len() > maximum {
        return Err(format!("{field} exceeds {maximum} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} contains control characters"));
    }
    Ok(value.to_owned())
}

fn validate_identities(
    identities: &[IdentityEvidenceV1],
    field: &str,
) -> Result<(), String> {
    if identities.len() > 128 {
        return Err(format!("{field} exceeds 128 values"));
    }
    for identity in identities {
        validate_text(&identity.kind, "identity kind", 64)?;
        validate_text(&identity.value, "identity value", 256)?;
    }
    Ok(())
}

fn validate_json_object(value: &Value, field: &str) -> Result<(), String> {
    let mut value_count = 0;
    validate_json_value(value, field, 1, &mut value_count)?;
    if matches!(value, Value::Null | Value::Object(_)) {
        Ok(())
    } else {
        Err(format!("{field} must be a JSON object"))
    }
}

fn validate_json_value(
    value: &Value,
    field: &str,
    depth: usize,
    value_count: &mut usize,
) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err(format!("{field} exceeds maximum depth {MAX_JSON_DEPTH}"));
    }
    *value_count += 1;
    if *value_count > MAX_JSON_VALUES {
        return Err(format!("{field} exceeds {MAX_JSON_VALUES} values"));
    }
    match value {
        Value::String(value) if value.len() > MAX_JSON_STRING_LEN => {
            Err(format!("{field} string exceeds {MAX_JSON_STRING_LEN} bytes"))
        }
        Value::Array(values) => {
            if values.len() > MAX_JSON_COLLECTION_LEN {
                return Err(format!(
                    "{field} array exceeds {MAX_JSON_COLLECTION_LEN} values"
                ));
            }
            for value in values {
                validate_json_value(value, field, depth + 1, value_count)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_JSON_COLLECTION_LEN {
                return Err(format!(
                    "{field} object exceeds {MAX_JSON_COLLECTION_LEN} fields"
                ));
            }
            for (key, value) in values {
                if key.len() > 256 {
                    return Err(format!("{field} key exceeds 256 bytes"));
                }
                validate_json_value(value, field, depth + 1, value_count)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySnapshotResultV1 {
    pub snapshot_id: String,
    pub replayed: bool,
    pub linked: usize,
    pub registered: usize,
    pub review_required: usize,
    pub missing: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_state_contracts_use_stable_snake_case_names() {
        assert_eq!(AssetLifecycle::WipePending.as_str(), "wipe_pending");
        assert_eq!(DiscoveryStatus::Manual.as_str(), "manual");
        assert_eq!(AssetCondition::Good.as_str(), "good");
        assert_eq!(CounterScope::ClassType.as_str(), "class_type");
        assert_eq!(IdentifierCase::Preserve.as_str(), "preserve");
        assert_eq!(
            DiscoveryPolicy::TrustedProviders.as_str(),
            "trusted_providers"
        );
        assert_eq!(
            serde_json::to_string(&AssetLifecycle::WipePending).unwrap(),
            "\"wipe_pending\""
        );
    }

    #[test]
    fn snapshot_defaults_optional_collections_and_payloads() {
        let snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "snapshot_id": "58b99686-7b8e-4d7f-a169-89cc56a6052c",
            "collector_version": "0.9.0",
            "platform": "linux",
            "collected_at": 1,
            "host": { "entity_key": "host" }
        }))
        .unwrap();

        assert!(snapshot.host.identities.is_empty());
        assert!(snapshot.entities.is_empty());
        assert_eq!(snapshot.host.attributes, Value::Null);
        assert_eq!(snapshot.host.runtime, Value::Null);
    }

    #[test]
    fn snapshot_validation_rejects_unknown_versions_and_duplicate_entity_keys() {
        let mut snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "snapshot_id": "58b99686-7b8e-4d7f-a169-89cc56a6052c",
            "collector_version": "0.9.0",
            "platform": "linux",
            "collected_at": 1,
            "host": { "entity_key": "host" },
            "entities": [
                { "entity_key": "disk-1", "entity_type": "physical_disk" },
                { "entity_key": "disk-1", "entity_type": "physical_disk" }
            ]
        }))
        .unwrap();

        assert!(snapshot.validate().is_err());
        snapshot.entities.clear();
        snapshot.schema_version = 2;
        assert!(snapshot.validate().is_err());
    }

    #[test]
    fn snapshot_validation_rejects_host_entity_collisions_and_non_object_payloads() {
        let mut snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "snapshot_id": "58b99686-7b8e-4d7f-a169-89cc56a6052c",
            "collector_version": "0.9.0",
            "platform": "linux",
            "collected_at": 1,
            "host": { "entity_key": "host", "attributes": [] },
            "entities": [{ "entity_key": "host", "entity_type": "physical_disk" }]
        }))
        .unwrap();

        assert!(snapshot.validate().is_err());
        snapshot.host.attributes = serde_json::json!({});
        assert!(snapshot.validate().is_err());
    }
}
