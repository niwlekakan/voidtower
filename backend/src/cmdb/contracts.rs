use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}
