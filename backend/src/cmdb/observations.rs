use crate::{
    audit::{self, PendingAudit},
    cmdb::{
        assets::MutationContext,
        contracts::{
            IdentityEvidenceV1, InventorySnapshotResultV1, InventorySnapshotV1, ObservedEntityV1,
        },
        correlation::{
            self, CorrelationOutcome, NormalizedEvidence, NormalizedIdentity, ReviewReason,
        },
        identifiers, relationships,
    },
    operations::{
        canonical_json,
        events::{self, PendingEvent},
        unix_now,
    },
};
use serde_json::{Map, Value};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

const MAX_ENTITIES: usize = 512;
const MAX_ENTITY_KEY_LEN: usize = 256;
const MAX_ENTITY_TYPE_LEN: usize = 64;
const MAX_COLLECTOR_VERSION_LEN: usize = 64;
const MAX_PLATFORM_LEN: usize = 32;
const MAX_WRITE_ATTEMPTS: usize = 4;
const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 8;
const MAX_JSON_VALUES: usize = 256;
const MAX_JSON_COLLECTION_LEN: usize = 128;
const MAX_JSON_STRING_LEN: usize = 4_096;

#[derive(Debug, thiserror::Error)]
pub enum ObservationError {
    #[error("invalid inventory snapshot: {0}")]
    Invalid(String),
    #[error("source CMDB asset not found")]
    SourceNotFound,
    #[error("CMDB asset not found")]
    AssetNotFound,
    #[error("enrolled node not found or not approved")]
    NodeNotTrusted,
    #[error("inventory snapshot is still processing")]
    Processing,
    #[error("observation not found")]
    ObservationNotFound,
    #[error("discovery conflict: {0}")]
    Conflict(String),
    #[error("observation operation failed")]
    Internal(#[source] anyhow::Error),
}

impl From<sqlx::Error> for ObservationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<anyhow::Error> for ObservationError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<serde_json::Error> for ObservationError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<correlation::CorrelationError> for ObservationError {
    fn from(error: correlation::CorrelationError) -> Self {
        match error {
            correlation::CorrelationError::Invalid(message) => Self::Invalid(message),
            error => Self::Internal(anyhow::Error::new(error)),
        }
    }
}

pub type Result<T> = std::result::Result<T, ObservationError>;

#[derive(Debug, Clone)]
pub struct IngestSnapshotInput {
    pub source_resource_id: String,
    pub node_id: Option<String>,
    pub provider: String,
    pub snapshot: InventorySnapshotV1,
}

#[derive(Debug, Clone, Default)]
pub struct RegisterDiscoveryInput {
    pub name: Option<String>,
    pub class_key: Option<String>,
    pub type_key: Option<String>,
    pub subtype: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ObservationRecord {
    pub id: String,
    pub resource_id: Option<String>,
    pub source_resource_id: String,
    pub snapshot_row_id: String,
    pub provider: String,
    pub scope_key: String,
    pub entity_key: String,
    pub entity_type: String,
    pub schema_version: i64,
    pub identity_json: String,
    pub attributes_json: String,
    pub runtime_json: String,
    pub health_json: String,
    pub provider_observed_at: Option<i64>,
    pub received_at: i64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub state: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExistingObservation {
    id: String,
    resource_id: Option<String>,
    state: String,
}

#[derive(Debug)]
struct EntityPersistence {
    resource_id: Option<String>,
    state: &'static str,
    registered: bool,
}

struct DiscoveredAssetInput<'a> {
    entity: &'a ObservedEntityV1,
    attributes: &'a Value,
    evidence: &'a NormalizedEvidence,
    provider: &'a str,
    registration: Option<&'a RegisterDiscoveryInput>,
    context: &'a MutationContext,
    now: i64,
}

struct SnapshotEntityContext<'a> {
    input: &'a IngestSnapshotInput,
    snapshot_row_id: &'a str,
    scope_key: &'a str,
    trusted: bool,
    policy: &'a str,
    context: &'a MutationContext,
    now: i64,
}

fn actor_type(context: &MutationContext) -> &'static str {
    context.actor.actor_type.as_str()
}

fn is_retryable(error: &ObservationError) -> bool {
    match error {
        ObservationError::Internal(error) => {
            let message = format!("{error:#}");
            message.contains("database is locked")
                || message.contains("database is busy")
                || message.contains("UNIQUE constraint failed: cmdb_inventory_snapshots")
        }
        _ => false,
    }
}

fn validate_required(value: &str, field: &str, maximum: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ObservationError::Invalid(format!("{field} is required")));
    }
    if value.len() > maximum {
        return Err(ObservationError::Invalid(format!(
            "{field} exceeds {maximum} bytes"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_token(value: &str, maximum: usize) -> Option<String> {
    let mut normalized = String::new();
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
        } else if matches!(character, ' ' | '-' | '_' | '.' | '/' | '"')
            && !normalized.ends_with('_')
        {
            normalized.push('_');
        } else if !matches!(character, ' ' | '-' | '_' | '.' | '/' | '"') {
            return None;
        }
    }
    let normalized = normalized.trim_matches('_').to_owned();
    (!normalized.is_empty() && normalized.len() <= maximum).then_some(normalized)
}

fn normalize_text(value: &Value, maximum: usize) -> Option<String> {
    let value = value
        .as_str()?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!value.is_empty() && value.len() <= maximum).then_some(value)
}

fn validate_json_structure(
    value: &Value,
    field: &str,
    depth: usize,
    value_count: &mut usize,
) -> Result<()> {
    if depth > MAX_JSON_DEPTH {
        return Err(ObservationError::Invalid(format!(
            "{field} exceeds maximum depth {MAX_JSON_DEPTH}"
        )));
    }
    *value_count += 1;
    if *value_count > MAX_JSON_VALUES {
        return Err(ObservationError::Invalid(format!(
            "{field} exceeds {MAX_JSON_VALUES} values"
        )));
    }
    match value {
        Value::String(value) if value.len() > MAX_JSON_STRING_LEN => {
            Err(ObservationError::Invalid(format!(
                "{field} string exceeds {MAX_JSON_STRING_LEN} bytes"
            )))
        }
        Value::Array(values) => {
            if values.len() > MAX_JSON_COLLECTION_LEN {
                return Err(ObservationError::Invalid(format!(
                    "{field} array exceeds {MAX_JSON_COLLECTION_LEN} values"
                )));
            }
            for value in values {
                validate_json_structure(value, field, depth + 1, value_count)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_JSON_COLLECTION_LEN {
                return Err(ObservationError::Invalid(format!(
                    "{field} object exceeds {MAX_JSON_COLLECTION_LEN} fields"
                )));
            }
            for (key, value) in values {
                if key.len() > 256 {
                    return Err(ObservationError::Invalid(format!(
                        "{field} key exceeds 256 bytes"
                    )));
                }
                validate_json_structure(value, field, depth + 1, value_count)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn normalized_attributes(attributes: &Value) -> Result<Value> {
    validate_json_structure(attributes, "entity attributes", 1, &mut 0)?;
    let mut attributes = match attributes {
        Value::Null => Map::new(),
        Value::Object(attributes) => attributes.clone(),
        _ => {
            return Err(ObservationError::Invalid(
                "entity attributes must be a JSON object".into(),
            ));
        }
    };
    if attributes.len() > 128 {
        return Err(ObservationError::Invalid(
            "entity attributes exceed 128 fields".into(),
        ));
    }
    for key in ["model", "manufacturer", "serial"] {
        if let Some(value) = attributes
            .get(key)
            .and_then(|value| normalize_text(value, 256))
        {
            attributes.insert(key.into(), Value::String(value));
        }
    }
    for key in ["interface", "protocol", "form_factor"] {
        if let Some(value) = attributes
            .get(key)
            .and_then(Value::as_str)
            .and_then(|value| normalize_token(value, 32))
        {
            attributes.insert(key.into(), Value::String(value));
        }
    }
    if let Some(value) = attributes.get("capacity_bytes") {
        let capacity = value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                ObservationError::Invalid("capacity_bytes must be a positive integer".into())
            })?;
        attributes.insert("capacity_bytes".into(), Value::from(capacity));
    }
    if let Some(value) = attributes.get("rotation") {
        let rotation = value
            .as_bool()
            .or_else(|| match value.as_str().map(str::trim) {
                Some("true" | "1") => Some(true),
                Some("false" | "0") => Some(false),
                _ => None,
            })
            .ok_or_else(|| ObservationError::Invalid("rotation must be a boolean".into()))?;
        attributes.insert("rotation".into(), Value::Bool(rotation));
    }
    let normalized = Value::Object(attributes);
    canonical_json::to_canonical_string(&normalized)
        .map_err(|error| ObservationError::Invalid(error.to_string()))?;
    Ok(normalized)
}

fn canonical_object(value: &Value, field: &str) -> Result<String> {
    validate_json_structure(value, field, 1, &mut 0)?;
    let value = match value {
        Value::Null => Value::Object(Map::new()),
        Value::Object(_) => value.clone(),
        _ => {
            return Err(ObservationError::Invalid(format!(
                "{field} must be a JSON object"
            )));
        }
    };
    canonical_json::to_canonical_string(&value)
        .map_err(|error| ObservationError::Invalid(error.to_string()))
}

fn augmented_identities(entity: &ObservedEntityV1, attributes: &Value) -> Vec<IdentityEvidenceV1> {
    let mut identities = entity.identities.clone();
    let existing: HashSet<String> = identities
        .iter()
        .map(|identity| identity.kind.trim().to_ascii_lowercase().replace('-', "_"))
        .collect();
    for kind in [
        "wwn",
        "nvme_uuid",
        "nvme_eui",
        "serial",
        "model",
        "hardware_uuid",
    ] {
        if !existing.contains(kind) {
            if let Some(value) = attributes.get(kind).and_then(Value::as_str) {
                identities.push(IdentityEvidenceV1 {
                    kind: kind.into(),
                    value: value.into(),
                });
            }
        }
    }
    identities
}

fn physical_type(attributes: &Value) -> (&'static str, Option<String>) {
    let rotation = attributes.get("rotation").and_then(Value::as_bool);
    let protocol = attributes
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let form_factor = attributes
        .get("form_factor")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if rotation == Some(true) {
        return ("hdd", None);
    }
    let subtype = (!protocol.is_empty()).then(|| protocol.to_owned());
    let type_key = match form_factor {
        "m_2" | "m2" => "ssdm2",
        "2_5" | "2_5_inch" => "ssd25",
        "u_2" | "u2" => "ssdu2",
        "u_3" | "u3" => "ssdu3",
        "pcie" => "ssdpcie",
        "msata" => "ssdms",
        _ if protocol == "nvme" => "ssd",
        _ => "ssd",
    };
    (type_key, subtype)
}

fn physical_type_known(attributes: &Value) -> bool {
    attributes
        .get("rotation")
        .and_then(Value::as_bool)
        .is_some()
        || attributes.get("protocol").and_then(Value::as_str) == Some("nvme")
}

fn display_name(entity: &ObservedEntityV1, attributes: &Value) -> String {
    let model = attributes.get("model").and_then(Value::as_str);
    let serial = attributes.get("serial").and_then(Value::as_str);
    match (model, serial) {
        (Some(model), Some(serial)) => format!("{model} {serial}"),
        (Some(model), None) => model.into(),
        _ => entity.entity_key.trim().into(),
    }
}

async fn append_event(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    event_type: &'static str,
    resource_id: Option<&str>,
    payload: Value,
) -> Result<()> {
    events::append(
        transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(context.actor.clone()),
            resource_id: resource_id.map(str::to_owned),
            job_id: None,
            approval_id: None,
            correlation_id: context.correlation_id.clone(),
            causation_id: None,
            payload,
        },
    )
    .await?;
    Ok(())
}

async fn append_audit(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    action: &'static str,
    resource_type: &'static str,
    resource_id: &str,
) -> Result<()> {
    audit::append(
        transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: actor_type(context),
            action,
            resource_type: Some(resource_type),
            resource_id: Some(resource_id),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: None,
            source: context.actor.source.as_deref(),
        },
    )
    .await?;
    Ok(())
}

async fn insert_identities(
    transaction: &mut Transaction<'_, Sqlite>,
    resource_id: &str,
    provider: &str,
    evidence: &NormalizedEvidence,
    now: i64,
) -> Result<()> {
    for identity in &evidence.identities {
        sqlx::query(
            "INSERT INTO cmdb_asset_identities \
             (id, resource_id, identity_kind, normalized_value, confidence, source, \
              first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(resource_id, identity_kind, normalized_value, source) \
             DO UPDATE SET last_seen_at = excluded.last_seen_at",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(resource_id)
        .bind(&identity.kind)
        .bind(&identity.value)
        .bind(&identity.confidence)
        .bind(provider)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn create_discovered_asset(
    transaction: &mut Transaction<'_, Sqlite>,
    input: DiscoveredAssetInput<'_>,
) -> Result<String> {
    let DiscoveredAssetInput {
        entity,
        attributes,
        evidence,
        provider,
        registration,
        context,
        now,
    } = input;
    let (physical_type_key, physical_subtype) = physical_type(attributes);
    let class_key = registration
        .and_then(|registration| registration.class_key.as_deref())
        .unwrap_or("hw")
        .trim();
    let type_key = registration
        .and_then(|registration| registration.type_key.as_deref())
        .unwrap_or(physical_type_key)
        .trim();
    identifiers::validate_key(class_key, "class")
        .map_err(|error| ObservationError::Invalid(error.to_string()))?;
    identifiers::validate_key(type_key, "type")
        .map_err(|error| ObservationError::Invalid(error.to_string()))?;
    let subtype = registration
        .and_then(|registration| registration.subtype.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or(physical_subtype);
    let asset_id = identifiers::allocate(transaction, class_key, type_key, now).await?;
    let resource_id = uuid::Uuid::new_v4().to_string();
    let name = registration
        .and_then(|registration| registration.name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| display_name(entity, attributes));
    if name.len() > 160 {
        return Err(ObservationError::Invalid(
            "asset name exceeds 160 bytes".into(),
        ));
    }
    let manufacturer = registration
        .and_then(|registration| registration.manufacturer.as_deref())
        .or_else(|| attributes.get("manufacturer").and_then(Value::as_str));
    let model = registration
        .and_then(|registration| registration.model.as_deref())
        .or_else(|| attributes.get("model").and_then(Value::as_str));
    let serial = registration
        .and_then(|registration| registration.serial_number.as_deref())
        .or_else(|| attributes.get("serial").and_then(Value::as_str));
    for (field, value) in [
        ("manufacturer", manufacturer),
        ("model", model),
        ("serial_number", serial),
        ("subtype", subtype.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().len() > 256) {
            return Err(ObservationError::Invalid(format!(
                "{field} exceeds 256 bytes"
            )));
        }
    }
    let metadata_json = canonical_json::to_canonical_string(&serde_json::json!({
        "discovered_by": provider,
        "physical": attributes,
    }))?;
    sqlx::query(
        "INSERT INTO resources \
         (id, kind, display_name, node_id, provider, lifecycle_state, revision, created_at, updated_at) \
         VALUES (?, 'cmdb_asset', ?, NULL, ?, 'active', 0, ?, ?)",
    )
    .bind(&resource_id)
    .bind(&name)
    .bind(provider)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO cmdb_assets \
         (resource_id, asset_id, class_key, type_key, subtype, manufacturer, model, serial_number, \
          lifecycle_status, discovery_status, condition_status, first_seen_at, last_seen_at, \
          metadata_json, notes, revision, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'inventory', 'online', 'unknown', ?, ?, ?, '', 0, ?, ?)",
    )
    .bind(&resource_id)
    .bind(&asset_id)
    .bind(class_key)
    .bind(type_key)
    .bind(subtype)
    .bind(manufacturer)
    .bind(model)
    .bind(serial)
    .bind(now)
    .bind(now)
    .bind(metadata_json)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO resource_aliases \
         (resource_id, namespace, scope_key, value, created_at, last_seen_at) \
         VALUES (?, 'cmdb.asset_id', 'global', ?, ?, ?)",
    )
    .bind(&resource_id)
    .bind(&asset_id)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    insert_identities(transaction, &resource_id, provider, evidence, now).await?;
    append_event(
        transaction,
        context,
        "cmdb.asset.created.v1",
        Some(&resource_id),
        serde_json::json!({
            "asset_id": asset_id,
            "class": class_key,
            "type": type_key,
            "source": if registration.is_some() { "discovery_review" } else { "trusted_observation" },
        }),
    )
    .await?;
    append_audit(
        transaction,
        context,
        "cmdb.asset.create",
        "asset",
        &resource_id,
    )
    .await?;
    Ok(resource_id)
}

async fn source_trust(
    transaction: &mut Transaction<'_, Sqlite>,
    source_resource_id: &str,
    node_id: Option<&str>,
    provider: &str,
) -> Result<bool> {
    let source: Option<Option<String>> = sqlx::query_scalar(
        "SELECT r.node_id FROM cmdb_assets a JOIN resources r ON r.id = a.resource_id \
         WHERE a.resource_id = ?",
    )
    .bind(source_resource_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let source_node_id = source.ok_or(ObservationError::SourceNotFound)?;
    if provider != "agent" {
        return Ok(false);
    }
    let node_id = node_id.ok_or(ObservationError::NodeNotTrusted)?;
    if source_node_id.as_deref() != Some(node_id) {
        return Err(ObservationError::NodeNotTrusted);
    }
    let trusted: Option<bool> =
        sqlx::query_scalar("SELECT approved = 1 AND agent_capable = 1 FROM nodes WHERE id = ?")
            .bind(node_id)
            .fetch_optional(&mut **transaction)
            .await?;
    trusted
        .filter(|trusted| *trusted)
        .ok_or(ObservationError::NodeNotTrusted)
}

async fn existing_observation(
    transaction: &mut Transaction<'_, Sqlite>,
    provider: &str,
    source_resource_id: &str,
    scope_key: &str,
    entity_key: &str,
) -> Result<Option<ExistingObservation>> {
    Ok(sqlx::query_as(
        "SELECT id, resource_id, state FROM cmdb_observations \
         WHERE provider = ? AND source_resource_id = ? AND scope_key = ? AND entity_key = ?",
    )
    .bind(provider)
    .bind(source_resource_id)
    .bind(scope_key)
    .bind(entity_key)
    .fetch_optional(&mut **transaction)
    .await?)
}

async fn ignored_fingerprint(
    transaction: &mut Transaction<'_, Sqlite>,
    observation_id: &str,
    fingerprint: &str,
) -> Result<bool> {
    let ignored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cmdb_discovery_decisions \
         WHERE observation_id = ? AND fingerprint = ? AND decision = 'ignored'",
    )
    .bind(observation_id)
    .bind(fingerprint)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(ignored != 0)
}

async fn update_asset_seen(
    transaction: &mut Transaction<'_, Sqlite>,
    resource_id: &str,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE cmdb_assets SET discovery_status = 'online', \
             first_seen_at = COALESCE(first_seen_at, ?), last_seen_at = ?, \
             revision = revision + 1, updated_at = ? WHERE resource_id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(resource_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn ensure_placement(
    transaction: &mut Transaction<'_, Sqlite>,
    resource_id: &str,
    source_resource_id: &str,
    provider: &str,
    scope_key: &str,
    entity_key: &str,
    context: &MutationContext,
) -> Result<()> {
    let current: Option<String> = sqlx::query_scalar(
        "SELECT destination_resource_id FROM cmdb_relationships \
         WHERE source_resource_id = ? AND type_key = 'installed_in' AND active = 1",
    )
    .bind(resource_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if current.as_deref() == Some(source_resource_id) {
        return Ok(());
    }
    relationships::start_in(
        transaction,
        &relationships::StartRelationshipInput {
            source_resource_id: resource_id.into(),
            destination_resource_id: source_resource_id.into(),
            type_key: "installed_in".into(),
            metadata: serde_json::json!({
                "provider": provider,
                "scope_key": scope_key,
                "entity_key": entity_key,
            }),
        },
        context,
    )
    .await
    .map_err(|error| ObservationError::Internal(anyhow::Error::new(error)))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_observation(
    transaction: &mut Transaction<'_, Sqlite>,
    observation_id: &str,
    resource_id: Option<&str>,
    snapshot_row_id: &str,
    provider: &str,
    source_resource_id: &str,
    scope_key: &str,
    entity: &ObservedEntityV1,
    identity_json: &str,
    attributes_json: &str,
    runtime_json: &str,
    health_json: &str,
    provider_observed_at: i64,
    fingerprint: &str,
    state: &str,
    received_at: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO cmdb_observations \
         (id, resource_id, source_resource_id, snapshot_row_id, provider, scope_key, entity_key, \
          entity_type, schema_version, identity_json, attributes_json, runtime_json, health_json, \
          provider_observed_at, received_at, first_seen_at, last_seen_at, state, fingerprint) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(provider, source_resource_id, scope_key, entity_key) DO UPDATE SET \
          resource_id = excluded.resource_id, snapshot_row_id = excluded.snapshot_row_id, \
          entity_type = excluded.entity_type, schema_version = excluded.schema_version, \
          identity_json = excluded.identity_json, attributes_json = excluded.attributes_json, \
          runtime_json = excluded.runtime_json, health_json = excluded.health_json, \
          provider_observed_at = excluded.provider_observed_at, received_at = excluded.received_at, \
          last_seen_at = excluded.last_seen_at, state = excluded.state, fingerprint = excluded.fingerprint",
    )
    .bind(observation_id)
    .bind(resource_id)
    .bind(source_resource_id)
    .bind(snapshot_row_id)
    .bind(provider)
    .bind(scope_key)
    .bind(entity.entity_key.trim())
    .bind(entity.entity_type.trim())
    .bind(identity_json)
    .bind(attributes_json)
    .bind(runtime_json)
    .bind(health_json)
    .bind(provider_observed_at)
    .bind(received_at)
    .bind(received_at)
    .bind(received_at)
    .bind(state)
    .bind(fingerprint)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_entity(
    transaction: &mut Transaction<'_, Sqlite>,
    work: &SnapshotEntityContext<'_>,
    entity: &ObservedEntityV1,
) -> Result<EntityPersistence> {
    let input = work.input;
    let snapshot_row_id = work.snapshot_row_id;
    let scope_key = work.scope_key;
    let trusted = work.trusted;
    let policy = work.policy;
    let context = work.context;
    let now = work.now;
    validate_required(&entity.entity_key, "entity_key", MAX_ENTITY_KEY_LEN)?;
    validate_required(&entity.entity_type, "entity_type", MAX_ENTITY_TYPE_LEN)?;
    let attributes = normalized_attributes(&entity.attributes)?;
    let runtime_json = canonical_object(&entity.runtime, "entity runtime")?;
    let health_json = canonical_object(&entity.health, "entity health")?;
    let identities = augmented_identities(entity, &attributes);
    let evidence = correlation::normalize(&input.provider, &identities)?;
    let identity_json = canonical_json::to_canonical_string(&evidence.identities)
        .map_err(|error| ObservationError::Invalid(error.to_string()))?;
    let attributes_json = canonical_json::to_canonical_string(&attributes)
        .map_err(|error| ObservationError::Invalid(error.to_string()))?;
    let fingerprint = canonical_json::digest(&serde_json::json!({
        "entity_type": entity.entity_type.trim(),
        "identities": evidence.identities,
        "malformed_strong_identity": evidence.malformed_strong_identity,
        "attributes": attributes,
    }))?;
    let existing = existing_observation(
        transaction,
        &input.provider,
        &input.source_resource_id,
        scope_key,
        entity.entity_key.trim(),
    )
    .await?;
    let observation_id = existing
        .as_ref()
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |row| row.id.clone());
    let suppressed = ignored_fingerprint(transaction, &observation_id, &fingerprint).await?;
    let outcome = correlation::correlate(transaction, &evidence).await?;
    let can_auto_register = entity.entity_type.trim() == "physical_disk"
        && physical_type_known(&attributes)
        && match policy {
            "automatic" => true,
            "trusted_providers" => trusted,
            _ => false,
        };
    let persistence = if suppressed || policy == "off" {
        EntityPersistence {
            resource_id: None,
            state: "ignored",
            registered: false,
        }
    } else {
        match outcome {
            CorrelationOutcome::Matched { resource_id } => EntityPersistence {
                resource_id: Some(resource_id),
                state: "online",
                registered: false,
            },
            CorrelationOutcome::UnmatchedStrong if can_auto_register => EntityPersistence {
                resource_id: Some(
                    create_discovered_asset(
                        transaction,
                        DiscoveredAssetInput {
                            entity,
                            attributes: &attributes,
                            evidence: &evidence,
                            provider: &input.provider,
                            registration: None,
                            context,
                            now,
                        },
                    )
                    .await?,
                ),
                state: "online",
                registered: true,
            },
            CorrelationOutcome::UnmatchedStrong | CorrelationOutcome::Review { .. } => {
                EntityPersistence {
                    resource_id: None,
                    state: "review",
                    registered: false,
                }
            }
        }
    };
    upsert_observation(
        transaction,
        &observation_id,
        persistence.resource_id.as_deref(),
        snapshot_row_id,
        &input.provider,
        &input.source_resource_id,
        scope_key,
        entity,
        &identity_json,
        &attributes_json,
        &runtime_json,
        &health_json,
        input.snapshot.collected_at,
        &fingerprint,
        persistence.state,
        now,
    )
    .await?;
    if let Some(resource_id) = persistence.resource_id.as_deref() {
        insert_identities(transaction, resource_id, &input.provider, &evidence, now).await?;
        update_asset_seen(transaction, resource_id, now).await?;
        if entity.entity_type.trim() == "physical_disk" {
            ensure_placement(
                transaction,
                resource_id,
                &input.source_resource_id,
                &input.provider,
                scope_key,
                entity.entity_key.trim(),
                context,
            )
            .await?;
        }
        if existing.as_ref().and_then(|row| row.resource_id.as_deref()) != Some(resource_id) {
            append_event(
                transaction,
                context,
                "cmdb.asset.observation_linked.v1",
                Some(resource_id),
                serde_json::json!({
                    "observation_id": observation_id,
                    "provider": input.provider,
                    "source_resource_id": input.source_resource_id,
                    "entity_key": entity.entity_key.trim(),
                }),
            )
            .await?;
        } else if existing
            .as_ref()
            .is_some_and(|row| row.state != persistence.state)
        {
            append_event(
                transaction,
                context,
                "cmdb.asset.observation_state_changed.v1",
                Some(resource_id),
                serde_json::json!({
                    "observation_id": observation_id,
                    "previous_state": existing.as_ref().map(|row| row.state.as_str()),
                    "state": persistence.state,
                }),
            )
            .await?;
        }
    }
    Ok(persistence)
}

async fn persist_host_observation(
    transaction: &mut Transaction<'_, Sqlite>,
    input: &IngestSnapshotInput,
    snapshot_row_id: &str,
    scope_key: &str,
    now: i64,
) -> Result<()> {
    let entity = ObservedEntityV1 {
        entity_key: validate_required(
            &input.snapshot.host.entity_key,
            "host.entity_key",
            MAX_ENTITY_KEY_LEN,
        )?,
        entity_type: "host".into(),
        identities: input.snapshot.host.identities.clone(),
        attributes: input.snapshot.host.attributes.clone(),
        runtime: input.snapshot.host.runtime.clone(),
        health: Value::Object(Map::new()),
    };
    let attributes = normalized_attributes(&entity.attributes)?;
    let evidence =
        correlation::normalize(&input.provider, &augmented_identities(&entity, &attributes))?;
    let identity_json = canonical_json::to_canonical_string(&evidence.identities)?;
    let attributes_json = canonical_json::to_canonical_string(&attributes)?;
    let runtime_json = canonical_object(&entity.runtime, "host runtime")?;
    let health_json = "{}";
    let fingerprint = canonical_json::digest(&serde_json::json!({
        "identities": evidence.identities,
        "attributes": attributes,
        "runtime": serde_json::from_str::<Value>(&runtime_json)?,
    }))?;
    let existing = existing_observation(
        transaction,
        &input.provider,
        &input.source_resource_id,
        scope_key,
        &entity.entity_key,
    )
    .await?;
    let observation_id = existing
        .as_ref()
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |row| row.id.clone());
    upsert_observation(
        transaction,
        &observation_id,
        Some(&input.source_resource_id),
        snapshot_row_id,
        &input.provider,
        &input.source_resource_id,
        scope_key,
        &entity,
        &identity_json,
        &attributes_json,
        &runtime_json,
        health_json,
        input.snapshot.collected_at,
        &fingerprint,
        "online",
        now,
    )
    .await?;
    update_asset_seen(transaction, &input.source_resource_id, now).await
}

async fn converge_missing(
    transaction: &mut Transaction<'_, Sqlite>,
    input: &IngestSnapshotInput,
    snapshot_row_id: &str,
    scope_key: &str,
    context: &MutationContext,
    now: i64,
) -> Result<usize> {
    let omitted: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, resource_id FROM cmdb_observations \
         WHERE provider = ? AND source_resource_id = ? AND scope_key = ? \
           AND snapshot_row_id != ? AND state = 'online' AND entity_type != 'host' \
           AND resource_id IS NOT NULL ORDER BY id",
    )
    .bind(&input.provider)
    .bind(&input.source_resource_id)
    .bind(scope_key)
    .bind(snapshot_row_id)
    .fetch_all(&mut **transaction)
    .await?;
    for (observation_id, resource_id) in &omitted {
        sqlx::query(
            "UPDATE cmdb_observations SET state = 'missing', received_at = ? \
             WHERE id = ?",
        )
        .bind(now)
        .bind(observation_id)
        .execute(&mut **transaction)
        .await?;
        let other_online_on_source: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_observations \
             WHERE resource_id = ? AND source_resource_id = ? AND state = 'online' AND id != ?",
        )
        .bind(resource_id)
        .bind(&input.source_resource_id)
        .bind(observation_id)
        .fetch_one(&mut **transaction)
        .await?;
        if other_online_on_source == 0 {
            let placement: Option<String> = sqlx::query_scalar(
                "SELECT id FROM cmdb_relationships WHERE source_resource_id = ? \
                 AND destination_resource_id = ? AND type_key = 'installed_in' AND active = 1",
            )
            .bind(resource_id)
            .bind(&input.source_resource_id)
            .fetch_optional(&mut **transaction)
            .await?;
            if let Some(relationship_id) = placement {
                relationships::end_in(transaction, &relationship_id, context)
                    .await
                    .map_err(|error| ObservationError::Internal(anyhow::Error::new(error)))?;
            }
        }
        let other_online: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_observations \
             WHERE resource_id = ? AND state = 'online' AND id != ?",
        )
        .bind(resource_id)
        .bind(observation_id)
        .fetch_one(&mut **transaction)
        .await?;
        if other_online == 0 {
            sqlx::query(
                "UPDATE cmdb_assets SET discovery_status = 'missing', revision = revision + 1, \
                 updated_at = ? WHERE resource_id = ?",
            )
            .bind(now)
            .bind(resource_id)
            .execute(&mut **transaction)
            .await?;
        }
        append_event(
            transaction,
            context,
            "cmdb.asset.observation_state_changed.v1",
            Some(resource_id),
            serde_json::json!({
                "observation_id": observation_id,
                "previous_state": "online",
                "state": "missing",
            }),
        )
        .await?;
    }
    Ok(omitted.len())
}

async fn replay(
    transaction: &mut Transaction<'_, Sqlite>,
    source_resource_id: &str,
    snapshot_id: &str,
    expected_fingerprint: &str,
) -> Result<Option<InventorySnapshotResultV1>> {
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT state, result_json, fingerprint FROM cmdb_inventory_snapshots \
         WHERE source_resource_id = ? AND snapshot_id = ?",
    )
    .bind(source_resource_id)
    .bind(snapshot_id)
    .fetch_optional(&mut **transaction)
    .await?;
    match row {
        None => Ok(None),
        Some((_, _, fingerprint)) if fingerprint != expected_fingerprint => {
            Err(ObservationError::Conflict(
                "snapshot_id was already used for different inventory content".into(),
            ))
        }
        Some((state, Some(result_json), _)) if state == "completed" => {
            let mut result: InventorySnapshotResultV1 = serde_json::from_str(&result_json)?;
            result.replayed = true;
            Ok(Some(result))
        }
        Some(_) => Err(ObservationError::Processing),
    }
}

async fn ingest_once(
    pool: &SqlitePool,
    input: &IngestSnapshotInput,
    context: &MutationContext,
) -> Result<InventorySnapshotResultV1> {
    if input.snapshot.schema_version != 1 {
        return Err(ObservationError::Invalid(
            "only inventory schema version 1 is supported".into(),
        ));
    }
    uuid::Uuid::parse_str(input.snapshot.snapshot_id.trim())
        .map_err(|_| ObservationError::Invalid("snapshot_id must be a UUID".into()))?;
    validate_required(&input.provider, "provider", 32)?;
    validate_required(
        &input.snapshot.collector_version,
        "collector_version",
        MAX_COLLECTOR_VERSION_LEN,
    )?;
    validate_required(&input.snapshot.platform, "platform", MAX_PLATFORM_LEN)?;
    if input.snapshot.collected_at <= 0 {
        return Err(ObservationError::Invalid(
            "collected_at must be a positive Unix timestamp".into(),
        ));
    }
    if input.snapshot.entities.len() > MAX_ENTITIES {
        return Err(ObservationError::Invalid(format!(
            "entity count exceeds {MAX_ENTITIES}"
        )));
    }
    let mut keys = HashSet::new();
    if input.snapshot.entities.iter().any(|entity| {
        let key = entity.entity_key.trim();
        key.is_empty() || !keys.insert(key.to_owned())
    }) {
        return Err(ObservationError::Invalid(
            "entity keys must be non-empty and unique within a snapshot".into(),
        ));
    }

    let snapshot_fingerprint =
        canonical_json::digest_with_limit(&input.snapshot, MAX_SNAPSHOT_BYTES)
            .map_err(|error| ObservationError::Invalid(error.to_string()))?;
    let mut transaction = pool.begin().await?;
    let trusted = source_trust(
        &mut transaction,
        &input.source_resource_id,
        input.node_id.as_deref(),
        input.provider.trim(),
    )
    .await?;
    if let Some(result) = replay(
        &mut transaction,
        &input.source_resource_id,
        input.snapshot.snapshot_id.trim(),
        &snapshot_fingerprint,
    )
    .await?
    {
        transaction.commit().await?;
        return Ok(result);
    }
    let policy: String = sqlx::query_scalar(
        "SELECT discovery_policy FROM cmdb_identifier_settings WHERE id = 'default'",
    )
    .fetch_one(&mut *transaction)
    .await?;
    let now = unix_now();
    let snapshot_row_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO cmdb_inventory_snapshots \
         (id, source_resource_id, node_id, snapshot_id, schema_version, collector_version, \
          platform, collected_at, received_at, state, fingerprint, result_json) \
         VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?, 'processing', ?, NULL)",
    )
    .bind(&snapshot_row_id)
    .bind(&input.source_resource_id)
    .bind(input.node_id.as_deref())
    .bind(input.snapshot.snapshot_id.trim())
    .bind(input.snapshot.collector_version.trim())
    .bind(input.snapshot.platform.trim())
    .bind(input.snapshot.collected_at)
    .bind(now)
    .bind(snapshot_fingerprint)
    .execute(&mut *transaction)
    .await?;
    let scope_key = input
        .node_id
        .as_deref()
        .unwrap_or(&input.source_resource_id);
    persist_host_observation(&mut transaction, input, &snapshot_row_id, scope_key, now).await?;

    let mut linked = 0;
    let mut registered = 0;
    let mut review_required = 0;
    let work = SnapshotEntityContext {
        input,
        snapshot_row_id: &snapshot_row_id,
        scope_key,
        trusted,
        policy: &policy,
        context,
        now,
    };
    for entity in &input.snapshot.entities {
        let persistence = persist_entity(&mut transaction, &work, entity).await?;
        if persistence.registered {
            registered += 1;
        } else if persistence.resource_id.is_some() {
            linked += 1;
        } else if persistence.state == "review" {
            review_required += 1;
        }
    }
    let missing = converge_missing(
        &mut transaction,
        input,
        &snapshot_row_id,
        scope_key,
        context,
        now,
    )
    .await?;
    let result = InventorySnapshotResultV1 {
        snapshot_id: input.snapshot.snapshot_id.trim().into(),
        replayed: false,
        linked,
        registered,
        review_required,
        missing,
    };
    let result_json = canonical_json::to_canonical_string(&result)?;
    sqlx::query(
        "UPDATE cmdb_inventory_snapshots SET state = 'completed', result_json = ? WHERE id = ?",
    )
    .bind(result_json)
    .bind(&snapshot_row_id)
    .execute(&mut *transaction)
    .await?;
    append_audit(
        &mut transaction,
        context,
        "cmdb.inventory.ingest",
        "inventory_snapshot",
        &snapshot_row_id,
    )
    .await?;
    transaction.commit().await?;
    Ok(result)
}

pub async fn ingest(
    pool: &SqlitePool,
    mut input: IngestSnapshotInput,
    context: MutationContext,
) -> Result<InventorySnapshotResultV1> {
    input.source_resource_id = validate_required(
        &input.source_resource_id,
        "source_resource_id",
        MAX_ENTITY_KEY_LEN,
    )?;
    input.provider = normalize_token(&input.provider, 32)
        .ok_or_else(|| ObservationError::Invalid("provider namespace is invalid".into()))?;
    input.node_id = input
        .node_id
        .as_deref()
        .map(|node_id| validate_required(node_id, "node_id", MAX_ENTITY_KEY_LEN))
        .transpose()?;
    for attempt in 0..MAX_WRITE_ATTEMPTS {
        match ingest_once(pool, &input, &context).await {
            Err(error) if is_retryable(&error) && attempt + 1 < MAX_WRITE_ATTEMPTS => {
                tokio::time::sleep(std::time::Duration::from_millis(
                    5 * u64::try_from(attempt + 1).unwrap_or(1),
                ))
                .await;
            }
            result => return result,
        }
    }
    unreachable!("bounded inventory ingestion attempts always return")
}

pub async fn list_discoveries(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ObservationRecord>> {
    Ok(sqlx::query_as(
        "SELECT id, resource_id, source_resource_id, snapshot_row_id, provider, scope_key, \
                entity_key, entity_type, schema_version, identity_json, attributes_json, \
                runtime_json, health_json, provider_observed_at, received_at, first_seen_at, \
                last_seen_at, state, fingerprint \
         FROM cmdb_observations WHERE resource_id IS NULL AND state IN ('review', 'ignored') \
         ORDER BY received_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?)
}

const OBSERVATION_SELECT: &str =
    "SELECT id, resource_id, source_resource_id, snapshot_row_id, provider, scope_key, \
            entity_key, entity_type, schema_version, identity_json, attributes_json, \
            runtime_json, health_json, provider_observed_at, received_at, first_seen_at, \
            last_seen_at, state, fingerprint FROM cmdb_observations";

pub async fn get_observation(
    pool: &SqlitePool,
    observation_id: &str,
) -> Result<Option<ObservationRecord>> {
    let sql = format!("{OBSERVATION_SELECT} WHERE id = ?");
    Ok(sqlx::query_as(&sql)
        .bind(observation_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn list_by_asset(
    pool: &SqlitePool,
    resource_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ObservationRecord>> {
    let sql = format!(
        "{OBSERVATION_SELECT} WHERE resource_id = ? \
         ORDER BY last_seen_at DESC, id DESC LIMIT ? OFFSET ?"
    );
    Ok(sqlx::query_as(&sql)
        .bind(resource_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
}

async fn observation_in(
    transaction: &mut Transaction<'_, Sqlite>,
    observation_id: &str,
) -> Result<ObservationRecord> {
    let sql = format!("{OBSERVATION_SELECT} WHERE id = ?");
    sqlx::query_as(&sql)
        .bind(observation_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(ObservationError::ObservationNotFound)
}

fn validate_decision_target(
    observation: &ObservationRecord,
    expected_fingerprint: &str,
) -> Result<()> {
    if observation.fingerprint != expected_fingerprint {
        return Err(ObservationError::Conflict(
            "observation fingerprint changed; review the current evidence".into(),
        ));
    }
    if observation.resource_id.is_some() {
        return Err(ObservationError::Conflict(
            "observation is already linked".into(),
        ));
    }
    if !matches!(observation.state.as_str(), "review" | "ignored") {
        return Err(ObservationError::Conflict(
            "observation is not awaiting a discovery decision".into(),
        ));
    }
    Ok(())
}

fn validate_notes(notes: Option<&str>) -> Result<Option<String>> {
    notes
        .map(str::trim)
        .filter(|notes| !notes.is_empty())
        .map(|notes| {
            if notes.len() > 2_000 {
                Err(ObservationError::Invalid(
                    "discovery notes exceed 2000 bytes".into(),
                ))
            } else {
                Ok(notes.to_owned())
            }
        })
        .transpose()
}

async fn insert_decision(
    transaction: &mut Transaction<'_, Sqlite>,
    observation: &ObservationRecord,
    decision: &'static str,
    notes: Option<&str>,
    context: &MutationContext,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO cmdb_discovery_decisions \
         (id, observation_id, fingerprint, decision, decided_by, notes, decided_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&observation.id)
    .bind(&observation.fingerprint)
    .bind(decision)
    .bind(context.actor.id.as_deref())
    .bind(notes)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
        {
            ObservationError::Conflict(
                "this evidence fingerprint already has a discovery decision".into(),
            )
        } else {
            ObservationError::from(error)
        }
    })?;
    Ok(())
}

pub async fn ignore_discovery(
    pool: &SqlitePool,
    observation_id: &str,
    expected_fingerprint: &str,
    notes: Option<&str>,
    context: MutationContext,
) -> Result<ObservationRecord> {
    let notes = validate_notes(notes)?;
    let mut transaction = pool.begin().await?;
    let observation = observation_in(&mut transaction, observation_id).await?;
    validate_decision_target(&observation, expected_fingerprint)?;
    if observation.state == "ignored"
        && ignored_fingerprint(&mut transaction, &observation.id, &observation.fingerprint).await?
    {
        transaction.commit().await?;
        return get_observation(pool, observation_id)
            .await?
            .ok_or(ObservationError::ObservationNotFound);
    }
    let now = unix_now();
    insert_decision(
        &mut transaction,
        &observation,
        "ignored",
        notes.as_deref(),
        &context,
        now,
    )
    .await?;
    sqlx::query("UPDATE cmdb_observations SET state = 'ignored' WHERE id = ?")
        .bind(observation_id)
        .execute(&mut *transaction)
        .await?;
    append_event(
        &mut transaction,
        &context,
        "cmdb.discovery.ignored.v1",
        Some(&observation.source_resource_id),
        serde_json::json!({
            "observation_id": observation.id,
            "fingerprint": observation.fingerprint,
            "provider": observation.provider,
            "entity_key": observation.entity_key,
        }),
    )
    .await?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.discovery.ignore",
        "discovery",
        observation_id,
    )
    .await?;
    transaction.commit().await?;
    get_observation(pool, observation_id)
        .await?
        .ok_or(ObservationError::ObservationNotFound)
}

fn evidence_from(observation: &ObservationRecord) -> Result<NormalizedEvidence> {
    let identities: Vec<NormalizedIdentity> = serde_json::from_str(&observation.identity_json)
        .map_err(|error| ObservationError::Internal(error.into()))?;
    Ok(NormalizedEvidence {
        identities,
        malformed_strong_identity: false,
    })
}

async fn ensure_identity_target_available(
    transaction: &mut Transaction<'_, Sqlite>,
    resource_id: &str,
    evidence: &NormalizedEvidence,
) -> Result<()> {
    for identity in evidence.identities.iter().filter(|identity| {
        matches!(
            identity.kind.as_str(),
            "wwn" | "nvme_uuid" | "nvme_eui" | "hardware_uuid"
        )
    }) {
        let conflict: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_asset_identities \
             WHERE identity_kind = ? AND normalized_value = ? AND resource_id != ?",
        )
        .bind(&identity.kind)
        .bind(&identity.value)
        .bind(resource_id)
        .fetch_one(&mut **transaction)
        .await?;
        if conflict != 0 {
            return Err(ObservationError::Conflict(format!(
                "{} identity is already linked to another asset",
                identity.kind
            )));
        }
    }
    Ok(())
}

async fn finalize_discovery_link(
    transaction: &mut Transaction<'_, Sqlite>,
    observation: &ObservationRecord,
    resource_id: &str,
    decision: &'static str,
    notes: Option<&str>,
    context: &MutationContext,
    now: i64,
) -> Result<()> {
    let evidence = evidence_from(observation)?;
    ensure_identity_target_available(transaction, resource_id, &evidence).await?;
    insert_identities(
        transaction,
        resource_id,
        &observation.provider,
        &evidence,
        now,
    )
    .await?;
    insert_decision(transaction, observation, decision, notes, context, now).await?;
    sqlx::query("UPDATE cmdb_observations SET resource_id = ?, state = 'online' WHERE id = ?")
        .bind(resource_id)
        .bind(&observation.id)
        .execute(&mut **transaction)
        .await?;
    update_asset_seen(transaction, resource_id, observation.last_seen_at).await?;
    if observation.entity_type == "physical_disk" {
        ensure_placement(
            transaction,
            resource_id,
            &observation.source_resource_id,
            &observation.provider,
            &observation.scope_key,
            &observation.entity_key,
            context,
        )
        .await?;
    }
    append_event(
        transaction,
        context,
        "cmdb.asset.observation_linked.v1",
        Some(resource_id),
        serde_json::json!({
            "observation_id": observation.id,
            "provider": observation.provider,
            "source_resource_id": observation.source_resource_id,
            "entity_key": observation.entity_key,
            "decision": decision,
        }),
    )
    .await?;
    append_audit(
        transaction,
        context,
        if decision == "linked" {
            "cmdb.discovery.link"
        } else {
            "cmdb.discovery.register"
        },
        "discovery",
        &observation.id,
    )
    .await
}

pub async fn link_discovery(
    pool: &SqlitePool,
    observation_id: &str,
    expected_fingerprint: &str,
    resource_id: &str,
    notes: Option<&str>,
    context: MutationContext,
) -> Result<ObservationRecord> {
    let notes = validate_notes(notes)?;
    let mut transaction = pool.begin().await?;
    let observation = observation_in(&mut transaction, observation_id).await?;
    validate_decision_target(&observation, expected_fingerprint)?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_assets WHERE resource_id = ?")
        .bind(resource_id)
        .fetch_one(&mut *transaction)
        .await?;
    if exists == 0 {
        return Err(ObservationError::AssetNotFound);
    }
    finalize_discovery_link(
        &mut transaction,
        &observation,
        resource_id,
        "linked",
        notes.as_deref(),
        &context,
        unix_now(),
    )
    .await?;
    transaction.commit().await?;
    get_observation(pool, observation_id)
        .await?
        .ok_or(ObservationError::ObservationNotFound)
}

pub async fn register_discovery(
    pool: &SqlitePool,
    observation_id: &str,
    expected_fingerprint: &str,
    registration: RegisterDiscoveryInput,
    notes: Option<&str>,
    context: MutationContext,
) -> Result<ObservationRecord> {
    let notes = validate_notes(notes)?;
    let mut transaction = pool.begin().await?;
    let observation = observation_in(&mut transaction, observation_id).await?;
    validate_decision_target(&observation, expected_fingerprint)?;
    let attributes: Value = serde_json::from_str(&observation.attributes_json)?;
    let evidence = evidence_from(&observation)?;
    match correlation::correlate(&mut transaction, &evidence).await? {
        CorrelationOutcome::Matched { .. } => {
            return Err(ObservationError::Conflict(
                "discovery evidence now matches an existing asset; link it instead".into(),
            ));
        }
        CorrelationOutcome::Review {
            reason: ReviewReason::AmbiguousIdentity | ReviewReason::ContradictoryIdentity,
        } => {
            return Err(ObservationError::Conflict(
                "ambiguous or contradictory evidence cannot register a new asset".into(),
            ));
        }
        CorrelationOutcome::UnmatchedStrong | CorrelationOutcome::Review { .. } => {}
    }
    let entity = ObservedEntityV1 {
        entity_key: observation.entity_key.clone(),
        entity_type: observation.entity_type.clone(),
        identities: Vec::new(),
        attributes,
        runtime: serde_json::from_str(&observation.runtime_json)?,
        health: serde_json::from_str(&observation.health_json)?,
    };
    let now = unix_now();
    let resource_id = create_discovered_asset(
        &mut transaction,
        DiscoveredAssetInput {
            entity: &entity,
            attributes: &entity.attributes,
            evidence: &evidence,
            provider: &observation.provider,
            registration: Some(&registration),
            context: &context,
            now,
        },
    )
    .await?;
    finalize_discovery_link(
        &mut transaction,
        &observation,
        &resource_id,
        "registered",
        notes.as_deref(),
        &context,
        now,
    )
    .await?;
    transaction.commit().await?;
    get_observation(pool, observation_id)
        .await?
        .ok_or(ObservationError::ObservationNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cmdb::assets::{self, CreateAssetInput},
        operations::contracts::{ActorRef, ActorType},
    };
    use sqlx::sqlite::SqlitePoolOptions;

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
        sqlx::query(
            "INSERT INTO users \
             (id, username, password_hash, role, force_password_change, created_at, updated_at) \
             VALUES ('owner', 'owner', 'test', 'owner', 0, 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn context(node_id: &str) -> MutationContext {
        MutationContext {
            actor: ActorRef {
                actor_type: ActorType::Node,
                id: Some(node_id.into()),
                source: Some("inventory_agent".into()),
            },
            correlation_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    async fn host(pool: &SqlitePool, node_id: &str, name: &str) -> String {
        sqlx::query(
            "INSERT INTO nodes \
             (id, display_name, device_type, owner_user_id, wg_public_key, token_hash, \
              agent_capable, approved, created_at) \
             VALUES (?, ?, 'server', 'owner', '', ?, 1, 1, 1)",
        )
        .bind(node_id)
        .bind(name)
        .bind(format!("token-{node_id}"))
        .execute(pool)
        .await
        .unwrap();
        let asset = assets::create_manual(
            pool,
            CreateAssetInput {
                class_key: "sys".into(),
                type_key: "host".into(),
                name: name.into(),
                friendly_name: None,
                description: None,
                manufacturer: None,
                model: None,
                serial_number: None,
                part_number: None,
                location_id: None,
                metadata: serde_json::json!({}),
                notes: String::new(),
            },
            context(node_id),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE resources SET node_id = ? WHERE id = ?")
            .bind(node_id)
            .bind(&asset.resource_id)
            .execute(pool)
            .await
            .unwrap();
        asset.resource_id
    }

    #[allow(clippy::too_many_arguments)]
    fn disk(
        entity_key: &str,
        wwn: Option<&str>,
        serial: Option<&str>,
        model: &str,
        rotation: bool,
        protocol: &str,
        form_factor: &str,
        path: &str,
    ) -> ObservedEntityV1 {
        let mut identities = vec![IdentityEvidenceV1 {
            kind: "model".into(),
            value: model.into(),
        }];
        if let Some(wwn) = wwn {
            identities.push(IdentityEvidenceV1 {
                kind: "wwn".into(),
                value: wwn.into(),
            });
        }
        if let Some(serial) = serial {
            identities.push(IdentityEvidenceV1 {
                kind: "serial".into(),
                value: serial.into(),
            });
        }
        ObservedEntityV1 {
            entity_key: entity_key.into(),
            entity_type: "physical_disk".into(),
            identities,
            attributes: serde_json::json!({
                "model": model,
                "serial": serial,
                "capacity_bytes": 1_000_000_u64,
                "rotation": rotation,
                "protocol": protocol,
                "form_factor": form_factor,
            }),
            runtime: serde_json::json!({"path": path}),
            health: serde_json::json!({"status": "ok"}),
        }
    }

    fn snapshot(platform: &str, entities: Vec<ObservedEntityV1>) -> InventorySnapshotV1 {
        InventorySnapshotV1 {
            schema_version: 1,
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            collector_version: "0.9.0-test".into(),
            platform: platform.into(),
            collected_at: 1,
            host: crate::cmdb::contracts::HostObservationV1 {
                entity_key: "host".into(),
                identities: vec![],
                attributes: serde_json::json!({}),
                runtime: serde_json::json!({}),
            },
            entities,
        }
    }

    fn input(
        host_resource_id: &str,
        node_id: &str,
        snapshot: InventorySnapshotV1,
    ) -> IngestSnapshotInput {
        IngestSnapshotInput {
            source_resource_id: host_resource_id.into(),
            node_id: Some(node_id.into()),
            provider: "agent".into(),
            snapshot,
        }
    }

    #[tokio::test]
    async fn trusted_strong_snapshot_registers_atomically_and_replay_is_side_effect_free() {
        let pool = pool().await;
        let node_id = "node-a";
        let host = host(&pool, node_id, "Host A").await;
        let snapshot = snapshot(
            "linux",
            vec![disk(
                "disk-by-path-a",
                Some("50:00:c5:00:ab:cd:12:34"),
                Some("SERIAL-A"),
                "Archive Disk",
                true,
                "sata",
                "3_5",
                "/dev/sdc",
            )],
        );
        let first = ingest(
            &pool,
            input(&host, node_id, snapshot.clone()),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(
            (first.registered, first.linked, first.review_required),
            (1, 0, 0)
        );
        let counts_before: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM cmdb_assets WHERE class_key = 'hw'), \
               (SELECT COUNT(*) FROM cmdb_asset_identities WHERE identity_kind = 'wwn'), \
               (SELECT COUNT(*) FROM cmdb_relationships WHERE type_key = 'installed_in'), \
               (SELECT COUNT(*) FROM events)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (counts_before.0, counts_before.1, counts_before.2),
            (1, 1, 1)
        );

        let mut conflicting_reuse = snapshot.clone();
        conflicting_reuse.platform = "windows".into();
        let replayed = ingest(&pool, input(&host, node_id, snapshot), context(node_id))
            .await
            .unwrap();
        assert!(replayed.replayed);
        assert_eq!(replayed.registered, 1);
        let counts_after: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM cmdb_assets WHERE class_key = 'hw'), \
               (SELECT COUNT(*) FROM cmdb_asset_identities WHERE identity_kind = 'wwn'), \
               (SELECT COUNT(*) FROM cmdb_relationships WHERE type_key = 'installed_in'), \
               (SELECT COUNT(*) FROM events)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts_after, counts_before);
        assert!(matches!(
            ingest(
                &pool,
                input(&host, node_id, conflicting_reuse),
                context(node_id)
            )
            .await,
            Err(ObservationError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn completed_snapshot_marks_omitted_disk_missing_without_deleting_it() {
        let pool = pool().await;
        let node_id = "node-missing";
        let host = host(&pool, node_id, "Missing Host").await;
        ingest(
            &pool,
            input(
                &host,
                node_id,
                snapshot(
                    "linux",
                    vec![disk(
                        "disk-a",
                        Some("50:00:c5:00:ab:cd:55:55"),
                        None,
                        "Disk",
                        true,
                        "sata",
                        "3_5",
                        "/dev/sda",
                    )],
                ),
            ),
            context(node_id),
        )
        .await
        .unwrap();
        let result = ingest(
            &pool,
            input(&host, node_id, snapshot("linux", vec![])),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(result.missing, 1);
        let disk: (String, String, String) = sqlx::query_as(
            "SELECT a.resource_id, a.discovery_status, o.state FROM cmdb_assets a \
             JOIN cmdb_observations o ON o.resource_id = a.resource_id \
             WHERE a.class_key = 'hw'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((disk.1.as_str(), disk.2.as_str()), ("missing", "missing"));
        let active_placement: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_relationships WHERE source_resource_id = ? \
             AND type_key = 'installed_in' AND active = 1",
        )
        .bind(disk.0)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(active_placement, 0);
    }

    #[tokio::test]
    async fn entity_key_change_on_same_host_keeps_current_placement() {
        let pool = pool().await;
        let node_id = "node-rekey";
        let host = host(&pool, node_id, "Rekey Host").await;
        let first_disk = disk(
            "provider-key-old",
            Some("50:00:c5:00:ab:cd:5a:01"),
            None,
            "Rekey Disk",
            true,
            "sata",
            "3_5",
            "/dev/sda",
        );
        ingest(
            &pool,
            input(&host, node_id, snapshot("linux", vec![first_disk])),
            context(node_id),
        )
        .await
        .unwrap();
        let second_disk = disk(
            "provider-key-new",
            Some("5000c500abcd5a01"),
            None,
            "Rekey Disk",
            true,
            "sata",
            "3_5",
            "/dev/sdz",
        );
        let result = ingest(
            &pool,
            input(&host, node_id, snapshot("linux", vec![second_disk])),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!((result.linked, result.missing), (1, 1));
        let state: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT a.discovery_status, \
               (SELECT COUNT(*) FROM cmdb_observations o \
                WHERE o.resource_id = a.resource_id AND o.state = 'online'), \
               (SELECT COUNT(*) FROM cmdb_observations o \
                WHERE o.resource_id = a.resource_id AND o.state = 'missing'), \
               (SELECT COUNT(*) FROM cmdb_relationships rel \
                WHERE rel.source_resource_id = a.resource_id \
                  AND rel.type_key = 'installed_in' AND rel.active = 1) \
             FROM cmdb_assets a WHERE a.class_key = 'hw'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, ("online".into(), 1, 1, 1));
    }

    #[tokio::test]
    async fn failed_reconciliation_rolls_back_and_same_snapshot_id_can_retry() {
        let pool = pool().await;
        let node_id = "node-retry";
        let host = host(&pool, node_id, "Retry Host").await;
        let good = disk(
            "good-disk",
            Some("50:00:c5:00:ab:cd:66:66"),
            None,
            "Good disk",
            true,
            "sata",
            "3_5",
            "/dev/sda",
        );
        let mut invalid = disk(
            "invalid-disk",
            Some("50:00:c5:00:ab:cd:77:77"),
            None,
            "Invalid disk",
            true,
            "sata",
            "3_5",
            "/dev/sdb",
        );
        invalid.attributes = Value::String("invalid".into());
        let failed_snapshot = snapshot("linux", vec![good.clone(), invalid]);
        let snapshot_id = failed_snapshot.snapshot_id.clone();
        let failed_context = context(node_id);
        let failed_correlation = failed_context.correlation_id.clone();
        assert!(ingest(
            &pool,
            input(&host, node_id, failed_snapshot),
            failed_context
        )
        .await
        .is_err());
        let residue: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM cmdb_inventory_snapshots WHERE snapshot_id = ?), \
               (SELECT COUNT(*) FROM cmdb_assets WHERE class_key = 'hw'), \
               (SELECT COUNT(*) FROM events WHERE correlation_id = ?), \
               (SELECT COUNT(*) FROM audit_log WHERE request_id = ?)",
        )
        .bind(&snapshot_id)
        .bind(&failed_correlation)
        .bind(&failed_correlation)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(residue, (0, 0, 0, 0));

        let mut retry = snapshot("linux", vec![good]);
        retry.snapshot_id = snapshot_id;
        let result = ingest(&pool, input(&host, node_id, retry), context(node_id))
            .await
            .unwrap();
        assert_eq!(result.registered, 1);
    }

    #[tokio::test]
    async fn path_independent_linux_to_windows_match_moves_disk_without_admin_overwrite() {
        let pool = pool().await;
        let host_a = host(&pool, "node-linux", "Linux Host").await;
        let host_b = host(&pool, "node-windows", "Windows Host").await;
        ingest(
            &pool,
            input(
                &host_a,
                "node-linux",
                snapshot(
                    "linux",
                    vec![disk(
                        "linux-path",
                        Some("50:00:c5:00:ab:cd:88:88"),
                        Some("MOVE-1"),
                        "Original Model",
                        false,
                        "sata",
                        "M.2",
                        "/dev/nvme0n1",
                    )],
                ),
            ),
            context("node-linux"),
        )
        .await
        .unwrap();
        let result = ingest(
            &pool,
            input(
                &host_b,
                "node-windows",
                snapshot(
                    "windows",
                    vec![disk(
                        "windows-disk-7",
                        Some("5000c500abcd8888"),
                        Some("MOVE-1"),
                        "Observed Changed Model",
                        false,
                        "nvme",
                        "M.2",
                        r"\\.\PhysicalDrive7",
                    )],
                ),
            ),
            context("node-windows"),
        )
        .await
        .unwrap();
        assert_eq!((result.linked, result.registered), (1, 0));
        let asset: (String, String, String, Option<String>) = sqlx::query_as(
            "SELECT a.resource_id, a.model, a.type_key, a.subtype FROM cmdb_assets a \
             WHERE a.class_key = 'hw'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(asset.1, "Original Model");
        assert_eq!(
            (asset.2.as_str(), asset.3.as_deref()),
            ("ssdm2", Some("sata"))
        );
        let destination: String = sqlx::query_scalar(
            "SELECT destination_resource_id FROM cmdb_relationships \
             WHERE source_resource_id = ? AND type_key = 'installed_in' AND active = 1",
        )
        .bind(&asset.0)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(destination, host_b);
    }

    #[tokio::test]
    async fn m2_sata_and_nvme_are_distinguished_by_protocol_not_form_factor() {
        let pool = pool().await;
        let node_id = "node-m2";
        let host = host(&pool, node_id, "M2 Host").await;
        let result = ingest(
            &pool,
            input(
                &host,
                node_id,
                snapshot(
                    "linux",
                    vec![
                        disk(
                            "m2-sata",
                            Some("50:00:c5:00:ab:cd:90:01"),
                            None,
                            "M2 SATA",
                            false,
                            "sata",
                            "M.2",
                            "/dev/sda",
                        ),
                        disk(
                            "m2-nvme",
                            Some("50:00:c5:00:ab:cd:90:02"),
                            None,
                            "M2 NVMe",
                            false,
                            "nvme",
                            "M.2",
                            "/dev/nvme0n1",
                        ),
                    ],
                ),
            ),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(result.registered, 2);
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT type_key, subtype FROM cmdb_assets WHERE class_key = 'hw' ORDER BY subtype",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                ("ssdm2".into(), Some("nvme".into())),
                ("ssdm2".into(), Some("sata".into()))
            ]
        );
    }

    #[tokio::test]
    async fn ignored_fingerprint_is_suppressed_until_material_evidence_changes() {
        let pool = pool().await;
        sqlx::query(
            "UPDATE cmdb_identifier_settings SET discovery_policy = 'review_first' \
             WHERE id = 'default'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let node_id = "node-ignore";
        let host = host(&pool, node_id, "Ignore Host").await;
        let first_entity = disk(
            "review-disk",
            Some("50:00:c5:00:ab:cd:a0:01"),
            None,
            "Review Disk",
            false,
            "sata",
            "2_5",
            "/dev/sda",
        );
        let first = ingest(
            &pool,
            input(
                &host,
                node_id,
                snapshot("linux", vec![first_entity.clone()]),
            ),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(first.review_required, 1);
        let discovery = list_discoveries(&pool, 20, 0).await.unwrap().remove(0);
        let ignored = ignore_discovery(
            &pool,
            &discovery.id,
            &discovery.fingerprint,
            Some("Known lab spare"),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(ignored.state, "ignored");
        ignore_discovery(
            &pool,
            &discovery.id,
            &discovery.fingerprint,
            None,
            context(node_id),
        )
        .await
        .unwrap();
        let decisions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_discovery_decisions WHERE observation_id = ?",
        )
        .bind(&discovery.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(decisions, 1);

        let mut runtime_changed = first_entity.clone();
        runtime_changed.runtime = serde_json::json!({"path": "/dev/sdz"});
        let unchanged_evidence = ingest(
            &pool,
            input(&host, node_id, snapshot("linux", vec![runtime_changed])),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(unchanged_evidence.review_required, 0);
        assert_eq!(
            get_observation(&pool, &discovery.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            "ignored"
        );

        let changed = disk(
            "review-disk",
            Some("50:00:c5:00:ab:cd:a0:02"),
            None,
            "Review Disk",
            false,
            "sata",
            "2_5",
            "/dev/sdz",
        );
        let reopened = ingest(
            &pool,
            input(&host, node_id, snapshot("linux", vec![changed])),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!(reopened.review_required, 1);
        let current = get_observation(&pool, &discovery.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.state, "review");
        assert_ne!(current.fingerprint, discovery.fingerprint);
        assert!(matches!(
            ignore_discovery(
                &pool,
                &discovery.id,
                &discovery.fingerprint,
                None,
                context(node_id)
            )
            .await,
            Err(ObservationError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn trusted_agent_weak_identity_stays_in_review() {
        let pool = pool().await;
        let node_id = "node-weak";
        let host = host(&pool, node_id, "Weak Host").await;
        let weak = ObservedEntityV1 {
            entity_key: "weak-disk".into(),
            entity_type: "physical_disk".into(),
            identities: vec![IdentityEvidenceV1 {
                kind: "serial".into(),
                value: "NON-UNIQUE-SERIAL".into(),
            }],
            attributes: serde_json::json!({
                "rotation": true,
                "capacity_bytes": 1_000_000_u64,
            }),
            runtime: serde_json::json!({"path": "/dev/sda"}),
            health: serde_json::json!({}),
        };
        let result = ingest(
            &pool,
            input(&host, node_id, snapshot("linux", vec![weak])),
            context(node_id),
        )
        .await
        .unwrap();
        assert_eq!((result.review_required, result.registered), (1, 0));
        let discovery = list_discoveries(&pool, 20, 0).await.unwrap().remove(0);
        assert_eq!(discovery.state, "review");
        assert!(discovery.resource_id.is_none());
    }

    #[tokio::test]
    async fn edit_and_register_creates_asset_and_links_discovery_transactionally() {
        let pool = pool().await;
        sqlx::query(
            "UPDATE cmdb_identifier_settings SET discovery_policy = 'review_first' \
             WHERE id = 'default'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let node_id = "node-register";
        let host = host(&pool, node_id, "Register Host").await;
        ingest(
            &pool,
            input(
                &host,
                node_id,
                snapshot(
                    "linux",
                    vec![disk(
                        "register-disk",
                        Some("50:00:c5:00:ab:cd:b0:01"),
                        Some("REGISTER-1"),
                        "Observed Model",
                        false,
                        "sata",
                        "2_5",
                        "/dev/sdb",
                    )],
                ),
            ),
            context(node_id),
        )
        .await
        .unwrap();
        let discovery = list_discoveries(&pool, 20, 0).await.unwrap().remove(0);
        let registered = register_discovery(
            &pool,
            &discovery.id,
            &discovery.fingerprint,
            RegisterDiscoveryInput {
                name: Some("Reviewed SSD".into()),
                model: Some("Administrator Model".into()),
                subtype: Some("sata_reviewed".into()),
                ..RegisterDiscoveryInput::default()
            },
            Some("Confirmed from purchase record"),
            context(node_id),
        )
        .await
        .unwrap();
        let resource_id = registered.resource_id.unwrap();
        assert_eq!(registered.state, "online");
        let asset: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT r.display_name, a.model, a.subtype FROM resources r \
             JOIN cmdb_assets a ON a.resource_id = r.id WHERE r.id = ?",
        )
        .bind(&resource_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            asset,
            (
                "Reviewed SSD".into(),
                Some("Administrator Model".into()),
                Some("sata_reviewed".into())
            )
        );
        let decision: String = sqlx::query_scalar(
            "SELECT decision FROM cmdb_discovery_decisions WHERE observation_id = ?",
        )
        .bind(&discovery.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let active_placement: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_relationships WHERE source_resource_id = ? \
             AND destination_resource_id = ? AND type_key = 'installed_in' AND active = 1",
        )
        .bind(resource_id)
        .bind(host)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((decision.as_str(), active_placement), ("registered", 1));
    }

    #[tokio::test]
    async fn link_existing_preserves_administrator_fields_and_records_decision() {
        let pool = pool().await;
        sqlx::query(
            "UPDATE cmdb_identifier_settings SET discovery_policy = 'review_first' \
             WHERE id = 'default'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let node_id = "node-link";
        let host = host(&pool, node_id, "Link Host").await;
        let existing = assets::create_manual(
            &pool,
            CreateAssetInput {
                class_key: "hw".into(),
                type_key: "ssd25".into(),
                name: "Administrator Disk".into(),
                friendly_name: None,
                description: Some("Do not overwrite".into()),
                manufacturer: None,
                model: Some("Administrator Model".into()),
                serial_number: Some("ADMIN-SERIAL".into()),
                part_number: None,
                location_id: None,
                metadata: serde_json::json!({"owner": "admin"}),
                notes: "kept".into(),
            },
            context(node_id),
        )
        .await
        .unwrap();
        ingest(
            &pool,
            input(
                &host,
                node_id,
                snapshot(
                    "linux",
                    vec![disk(
                        "link-disk",
                        Some("50:00:c5:00:ab:cd:c0:01"),
                        Some("OBSERVED-SERIAL"),
                        "Observed Model",
                        false,
                        "sata",
                        "2_5",
                        "/dev/sdc",
                    )],
                ),
            ),
            context(node_id),
        )
        .await
        .unwrap();
        let discovery = list_discoveries(&pool, 20, 0).await.unwrap().remove(0);
        link_discovery(
            &pool,
            &discovery.id,
            &discovery.fingerprint,
            &existing.resource_id,
            None,
            context(node_id),
        )
        .await
        .unwrap();
        let after = assets::get(&pool, &existing.resource_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.display_name, "Administrator Disk");
        assert_eq!(after.model.as_deref(), Some("Administrator Model"));
        assert_eq!(after.serial_number.as_deref(), Some("ADMIN-SERIAL"));
        assert_eq!(after.description.as_deref(), Some("Do not overwrite"));
        assert_eq!(after.notes, "kept");
        let decision: String = sqlx::query_scalar(
            "SELECT decision FROM cmdb_discovery_decisions WHERE observation_id = ?",
        )
        .bind(discovery.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(decision, "linked");
    }

    #[tokio::test]
    async fn simultaneous_host_evidence_creates_one_asset_and_one_active_placement() {
        let db_path = std::env::temp_dir().join(format!(
            "voidtower-cmdb-observations-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::init_pool(&db_path).await.unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO users \
             (id, username, password_hash, role, force_password_change, created_at, updated_at) \
             VALUES ('owner', 'owner', 'test', 'owner', 0, 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let host_a = host(&pool, "node-race-a", "Race Host A").await;
        let host_b = host(&pool, "node-race-b", "Race Host B").await;
        let first = input(
            &host_a,
            "node-race-a",
            snapshot(
                "linux",
                vec![disk(
                    "race-linux",
                    Some("50:00:c5:00:ab:cd:d0:01"),
                    None,
                    "Race Disk",
                    true,
                    "sata",
                    "3_5",
                    "/dev/sda",
                )],
            ),
        );
        let second = input(
            &host_b,
            "node-race-b",
            snapshot(
                "windows",
                vec![disk(
                    "race-windows",
                    Some("5000c500abcdd001"),
                    None,
                    "Race Disk",
                    true,
                    "sata",
                    "3_5",
                    r"\\.\PhysicalDrive2",
                )],
            ),
        );
        let (first, second) = tokio::join!(
            ingest(&pool, first, context("node-race-a")),
            ingest(&pool, second, context("node-race-b"))
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_eq!(first.registered + second.registered, 1);
        assert_eq!(first.linked + second.linked, 1);
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT \
               (SELECT COUNT(*) FROM cmdb_assets WHERE class_key = 'hw'), \
               (SELECT COUNT(*) FROM cmdb_observations WHERE entity_type = 'physical_disk'), \
               (SELECT COUNT(*) FROM cmdb_relationships \
                WHERE type_key = 'installed_in' AND active = 1)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 2, 1));
        pool.close().await;
        for path in [
            db_path.clone(),
            db_path.with_extension("db-shm"),
            db_path.with_extension("db-wal"),
            db_path.with_extension("db.migration.lock"),
        ] {
            let _ = std::fs::remove_file(path);
        }
    }
}
