//! Windows physical-disk evidence projected onto the platform-neutral
//! [`InventorySnapshotV1`] contract.
//!
//! The evidence source is declared once in [`WINDOWS_PHYSICAL_DISK_COMMAND`] so
//! the runner (W4-02/W4-03) cannot invent a second shape. Parsing is pure: it
//! needs no database, no network, and no Windows host, which is why the contract
//! and its fixtures can be `unit-verified` before the target gate exists.
//!
//! Same semantics as the Linux collector: one entity per physical disk, stable
//! identity evidence required, virtual/loopback storage excluded, bounded input,
//! bounded entity count, bounded strings, and a failed parse returns an error
//! instead of a partial or empty "full inventory" snapshot. `agent::supervision`
//! already skips the upload when collection returns an error.

use super::{
    bounded, check_depth, check_value_strings, positive_integer, CollectorError, MAX_ENTITY_COUNT,
    MAX_STRING_BYTES,
};
use crate::cmdb::contracts::{
    HostObservationV1, IdentityEvidenceV1, InventorySnapshotV1, ObservedEntityV1,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Bumped when the Windows evidence mapping changes shape.
pub const WINDOWS_COLLECTOR_VERSION: &str = "windows-collector-v1";

/// The declared Windows evidence command. Every field the parser reads must be
/// selected here; `the_declared_evidence_command_selects_every_field_the_parser_reads`
/// enforces that agreement. `-NoProfile -NonInteractive` keeps the invocation
/// independent of operator shell profiles.
pub const WINDOWS_PHYSICAL_DISK_COMMAND: &str = concat!(
    "powershell.exe -NoProfile -NonInteractive -Command \"",
    "ConvertTo-Json -Depth 4 -InputObject @{ PhysicalDisks = @(Get-PhysicalDisk | Select-Object ",
    "DeviceId,FriendlyName,SerialNumber,UniqueId,MediaType,BusType,Size,",
    "HealthStatus,OperationalStatus,PhysicalLocation) }\""
);

/// Upper bound for the whole report, matching the Linux collector's order of
/// magnitude. The report is produced by one bounded command, never streamed.
pub const MAX_WINDOWS_REPORT_BYTES: usize = 256 * 1024;

/// Serial numbers Windows firmware reports for "no serial number". They must
/// never become identity evidence, because every such disk would collide.
const WINDOWS_PLACEHOLDER_SERIALS: &[&str] = &[
    "to be filled by o.e.m.",
    "to be filled by oem",
    "default string",
    "unknown",
    "unspecified",
    "none",
    "n/a",
    "na",
    "not specified",
    "not available",
    "invalid",
];

/// The identity value bound of `cmdb::contracts` is tighter than this module's
/// string bound, so an unusable-long identifier is rejected with a precise
/// error instead of failing contract validation later.
const MAX_IDENTITY_VALUE_BYTES: usize = 256;

/// Parse a sanitized Windows disk report with fixed snapshot metadata.
pub fn collect_windows_fixture(raw: &str) -> Result<InventorySnapshotV1, CollectorError> {
    collect_windows_snapshot(
        raw,
        "00000000-0000-0000-0000-000000000002",
        1_700_000_000,
        "host:windows-fixture",
    )
}

/// Parse a Windows disk report into the platform-neutral snapshot contract.
///
/// `raw` is the JSON document [`WINDOWS_PHYSICAL_DISK_COMMAND`] produces:
/// `{"PhysicalDisks": [ ... ]}`. A report that cannot be parsed, exceeds a
/// bound, or contains a disk without stable identity is an error — never a
/// degraded snapshot.
pub fn collect_windows_snapshot(
    raw: &str,
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
) -> Result<InventorySnapshotV1, CollectorError> {
    if raw.is_empty() {
        return Err(CollectorError::Empty);
    }
    if raw.len() > MAX_WINDOWS_REPORT_BYTES {
        return Err(CollectorError::Oversized);
    }
    if snapshot_id.len() > MAX_STRING_BYTES {
        return Err(CollectorError::OversizedField("snapshot_id"));
    }
    if host_key.len() > MAX_STRING_BYTES {
        return Err(CollectorError::OversizedField("host_key"));
    }
    let report: Value = serde_json::from_str(raw).map_err(|_| CollectorError::InvalidJson)?;
    check_depth(&report, 0)?;
    check_value_strings(&report, "windows-disks", 0)?;
    let disks = report
        .get("PhysicalDisks")
        .and_then(Value::as_array)
        .ok_or(CollectorError::MissingDisks)?;
    let mut entities = Vec::new();
    let mut entity_keys = HashSet::new();
    let mut identity_keys = HashSet::new();
    for disk in disks {
        collect_disk(disk, &mut entities, &mut entity_keys, &mut identity_keys)?;
    }
    let snapshot = InventorySnapshotV1 {
        schema_version: 1,
        snapshot_id: snapshot_id.into(),
        collector_version: WINDOWS_COLLECTOR_VERSION.into(),
        platform: "windows".into(),
        collected_at,
        host: HostObservationV1 {
            entity_key: host_key.into(),
            identities: Vec::new(),
            attributes: serde_json::json!({ "source": "windows" }),
            runtime: serde_json::json!({ "collection": "complete" }),
        },
        entities,
    };
    snapshot
        .validate()
        .map_err(CollectorError::InvalidSnapshot)?;
    Ok(snapshot)
}

fn collect_disk(
    disk: &Value,
    out: &mut Vec<ObservedEntityV1>,
    entity_keys: &mut HashSet<String>,
    identity_keys: &mut HashSet<String>,
) -> Result<(), CollectorError> {
    let raw_bus = bounded(disk, "BusType")?.unwrap_or_default();
    let bus = normalize_token(&raw_bus);
    // Storage Spaces and loopback volumes present themselves as disks but are
    // not physical hardware. They are excluded like the Linux collector
    // excludes `loop`/`ram`/`rom`/`part` entries.
    if matches!(bus.as_str(), "virtual" | "file_backed_virtual") {
        return Ok(());
    }
    if out.len() >= MAX_ENTITY_COUNT {
        return Err(CollectorError::TooManyEntities);
    }

    let friendly_name = bounded(disk, "FriendlyName")?;
    let serial = usable_serial(bounded(disk, "SerialNumber")?)?;
    let unique_id = usable_identifier(bounded(disk, "UniqueId")?, "UniqueId")?;
    let media_type = bounded(disk, "MediaType")?
        .map(|value| normalize_token(&value))
        .filter(|value| !value.is_empty());
    let capacity_bytes = positive_integer(disk, "Size")?;

    let mut identities = Vec::new();
    if let Some(serial) = serial.as_ref() {
        identities.push(IdentityEvidenceV1 {
            kind: "serial".into(),
            value: serial.clone(),
        });
    }
    // Windows storage identifiers (for example `{eui.…}`) are reported by this
    // platform only. They are evidence, never a canonical resource UUID, and
    // this contract/fixture slice deliberately does not promote them to a
    // strong cross-platform identity kind.
    if let Some(unique_id) = unique_id.as_ref() {
        identities.push(IdentityEvidenceV1 {
            kind: "windows_unique_id".into(),
            value: unique_id.clone(),
        });
    }
    for identity in &identities {
        if !identity_keys.insert(format!("{}:{}", identity.kind, identity.value)) {
            return Err(CollectorError::IdentityCollision);
        }
    }
    let entity_key = format!(
        "physical-disk:{}",
        serial
            .as_deref()
            .or(unique_id.as_deref())
            .ok_or(CollectorError::MissingIdentity)?
    );
    if !entity_keys.insert(entity_key.clone()) {
        return Err(CollectorError::IdentityCollision);
    }

    let mut attributes = Map::new();
    if let Some(name) = friendly_name.as_ref() {
        attributes.insert("name".into(), Value::String(name.clone()));
        attributes.insert("model".into(), Value::String(name.clone()));
    }
    if let Some(serial) = serial.as_ref() {
        attributes.insert("serial".into(), Value::String(serial.clone()));
    }
    attributes.insert("capacity_bytes".into(), Value::from(capacity_bytes));
    if !bus.is_empty() {
        attributes.insert("protocol".into(), Value::String(bus.clone()));
        // USB enclosures are the only removable class this evidence names.
        attributes.insert("removable".into(), Value::Bool(bus == "usb"));
    }
    if let Some(media_type) = media_type.as_ref() {
        attributes.insert("media_type".into(), Value::String(media_type.clone()));
        // Rotation is knowledge about the physical medium, not an inference
        // from capacity, so it is only stated when Windows states it.
        if let Some(rotation) = rotation_of(media_type) {
            attributes.insert("rotation".into(), Value::Bool(rotation));
        }
    }

    let mut runtime = Map::new();
    if let Some(device_id) = bounded(disk, "DeviceId")? {
        runtime.insert("device_id".into(), Value::String(device_id));
    }
    if let Some(location) = bounded(disk, "PhysicalLocation")? {
        runtime.insert("physical_location".into(), Value::String(location));
    }
    if !raw_bus.trim().is_empty() {
        runtime.insert("bus_type".into(), Value::String(raw_bus));
    }
    // OperationalStatus is a CIM string array; it stays a bounded observation
    // in runtime rather than a normalized attribute.
    if let Some(status) = disk
        .get("OperationalStatus")
        .filter(|value| !value.is_null())
    {
        check_value_strings(status, "OperationalStatus", 0)?;
        runtime.insert("operational_status".into(), status.clone());
    }

    let mut health = Map::new();
    if let Some(status) = bounded(disk, "HealthStatus")? {
        health.insert("status".into(), Value::String(status.to_ascii_lowercase()));
    }

    out.push(ObservedEntityV1 {
        entity_key,
        entity_type: "physical_disk".into(),
        identities,
        attributes: Value::Object(attributes),
        runtime: Value::Object(runtime),
        health: Value::Object(health),
    });
    Ok(())
}

fn rotation_of(media_type: &str) -> Option<bool> {
    match media_type {
        "hdd" => Some(true),
        "ssd" | "scm" => Some(false),
        _ => None,
    }
}

/// Lowercase ASCII token for a Windows CIM enum value, because the CMDB
/// projection keeps single tokens for `protocol`/`media_type`.
fn normalize_token(value: &str) -> String {
    let mut token = String::new();
    let mut separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            token.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !token.is_empty() {
            token.push('_');
            separator = true;
        }
    }
    while token.ends_with('_') {
        token.pop();
    }
    token
}

/// Trim firmware padding and reject values that cannot be identity evidence.
fn usable_identifier(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<String>, CollectorError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed =
        raw.trim_matches(|character: char| character == '\0' || character.is_whitespace());
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_IDENTITY_VALUE_BYTES {
        return Err(CollectorError::OversizedField(field));
    }
    if trimmed.chars().any(char::is_control) {
        return Ok(None);
    }
    Ok(Some(trimmed.to_owned()))
}

fn usable_serial(value: Option<String>) -> Result<Option<String>, CollectorError> {
    let identifier = usable_identifier(value, "SerialNumber")?;
    match identifier {
        Some(value) if is_placeholder_serial(&value) => Ok(None),
        other => Ok(other),
    }
}

fn is_placeholder_serial(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    WINDOWS_PLACEHOLDER_SERIALS.contains(&lowered.as_str())
        || lowered.chars().all(|character| character == '0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmdb::contracts::IdentityEvidenceV1;
    use crate::collector::MAX_JSON_DEPTH;
    use serde_json::json;

    const FIXTURE: &str =
        include_str!("../../tests/fixtures/windows-collector/physical-disks.json");
    const NO_IDENTITY_FIXTURE: &str = include_str!(
        "../../tests/fixtures/windows-collector/physical-disks-no-stable-identity.json"
    );

    fn disk(serial: &str, unique_id: &str) -> Value {
        json!({
            "DeviceId": "0",
            "FriendlyName": "Fixture Disk",
            "SerialNumber": serial,
            "UniqueId": unique_id,
            "MediaType": "SSD",
            "BusType": "SATA",
            "Size": 100000000000u64,
            "HealthStatus": "Healthy",
            "OperationalStatus": "OK",
            "PhysicalLocation": "Integrated"
        })
    }

    fn virtual_disk() -> Value {
        json!({
            "DeviceId": "3",
            "FriendlyName": "Fixture Virtual Disk",
            "SerialNumber": "",
            "UniqueId": "{fixture-virtual-0001}",
            "MediaType": "Unspecified",
            "BusType": "Virtual",
            "Size": 400000000000u64,
            "HealthStatus": "Healthy",
            "OperationalStatus": "OK",
            "PhysicalLocation": "Virtual"
        })
    }

    #[test]
    fn windows_fixture_produces_the_platform_neutral_contract() {
        let s = collect_windows_fixture(FIXTURE).unwrap();
        assert_eq!(s.schema_version, 1);
        assert_eq!(s.platform, "windows");
        assert_eq!(s.collector_version, WINDOWS_COLLECTOR_VERSION);
        assert_eq!(s.snapshot_id, "00000000-0000-0000-0000-000000000002");
        assert_eq!(s.collected_at, 1_700_000_000);
        assert_eq!(s.host.entity_key, "host:windows-fixture");
        assert_eq!(s.host.attributes["source"], "windows");
        assert_eq!(s.host.runtime["collection"], "complete");
        assert_eq!(s.entities.len(), 3);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn physical_disk_identity_and_attributes_match_the_linux_projection() {
        let s = collect_windows_fixture(FIXTURE).unwrap();

        let hdd = &s.entities[0];
        assert_eq!(hdd.entity_type, "physical_disk");
        assert_eq!(hdd.entity_key, "physical-disk:FIXTURE-HDD-0001");
        assert_eq!(
            hdd.identities,
            vec![
                IdentityEvidenceV1 {
                    kind: "serial".into(),
                    value: "FIXTURE-HDD-0001".into()
                },
                IdentityEvidenceV1 {
                    kind: "windows_unique_id".into(),
                    value: "2000FIXTUREHDD0001".into()
                }
            ]
        );
        assert_eq!(hdd.attributes["name"], "Fixture SATA HDD");
        assert_eq!(hdd.attributes["model"], "Fixture SATA HDD");
        assert_eq!(hdd.attributes["serial"], "FIXTURE-HDD-0001");
        assert_eq!(hdd.attributes["capacity_bytes"], 100000000000u64);
        assert_eq!(hdd.attributes["protocol"], "sata");
        assert_eq!(hdd.attributes["rotation"], true);
        assert_eq!(hdd.attributes["media_type"], "hdd");
        assert_eq!(hdd.attributes["removable"], false);
        assert_eq!(hdd.health["status"], "healthy");
        assert_eq!(hdd.runtime["device_id"], "0");
        assert_eq!(hdd.runtime["bus_type"], "SATA");

        let nvme = &s.entities[1];
        assert_eq!(nvme.entity_key, "physical-disk:FIXTURE-NVME-0001");
        assert_eq!(nvme.attributes["protocol"], "nvme");
        assert_eq!(nvme.attributes["rotation"], false);
        assert_eq!(nvme.attributes["media_type"], "ssd");

        let usb = &s.entities[2];
        assert_eq!(usb.entity_key, "physical-disk:FIXTURE-USB-0001");
        assert_eq!(usb.attributes["removable"], true);
        assert_eq!(usb.attributes["capacity_bytes"], 2000000000000u64);
        // A CIM string array stays an observation, bounded like every other value.
        assert_eq!(usb.runtime["operational_status"], json!(["Degraded"]));
    }

    #[test]
    fn virtual_storage_is_excluded_without_failing_the_snapshot() {
        let s = collect_windows_fixture(FIXTURE).unwrap();
        assert!(s
            .entities
            .iter()
            .all(|entity| !entity.entity_key.contains("virtual")));

        let only_virtual = json!({ "PhysicalDisks": [virtual_disk()] }).to_string();
        let s = collect_windows_fixture(&only_virtual).unwrap();
        assert!(s.entities.is_empty());
        // A host-only snapshot is valid; it never implies missing assets.
        assert!(s.validate().is_ok());
    }

    #[test]
    fn placeholder_serial_falls_back_to_the_windows_unique_id() {
        let raw = json!({
            "PhysicalDisks": [disk("  To be filled by O.E.M.  ", "{eui.fixture0000000009}")]
        })
        .to_string();
        let s = collect_windows_fixture(&raw).unwrap();
        assert_eq!(s.entities.len(), 1);
        assert_eq!(
            s.entities[0].entity_key,
            "physical-disk:{eui.fixture0000000009}"
        );
        assert_eq!(
            s.entities[0].identities,
            vec![IdentityEvidenceV1 {
                kind: "windows_unique_id".into(),
                value: "{eui.fixture0000000009}".into()
            }]
        );
        assert!(s.entities[0].attributes.get("serial").is_none());
    }

    #[test]
    fn a_disk_without_stable_identity_fails_the_whole_collection() {
        assert_eq!(
            collect_windows_fixture(NO_IDENTITY_FIXTURE),
            Err(CollectorError::MissingIdentity)
        );
    }

    #[test]
    fn duplicate_identity_collides() {
        let repeated_serial = json!({
            "PhysicalDisks": [disk("SAME-0001", "U-0001"), disk("SAME-0001", "U-0002")]
        })
        .to_string();
        assert_eq!(
            collect_windows_fixture(&repeated_serial),
            Err(CollectorError::IdentityCollision)
        );

        let shared_unique_id = json!({
            "PhysicalDisks": [disk("A-0001", "SAME"), disk("B-0001", "SAME")]
        })
        .to_string();
        assert_eq!(
            collect_windows_fixture(&shared_unique_id),
            Err(CollectorError::IdentityCollision)
        );
    }

    #[test]
    fn report_bounds_match_the_linux_collector() {
        assert_eq!(collect_windows_fixture(""), Err(CollectorError::Empty));
        assert_eq!(
            collect_windows_fixture("{"),
            Err(CollectorError::InvalidJson)
        );
        assert_eq!(
            collect_windows_fixture(&"x".repeat(MAX_WINDOWS_REPORT_BYTES + 1)),
            Err(CollectorError::Oversized)
        );
        assert_eq!(
            collect_windows_fixture("{}"),
            Err(CollectorError::MissingDisks)
        );
        assert_eq!(
            collect_windows_fixture(r#"{"PhysicalDisks":{}}"#),
            Err(CollectorError::MissingDisks)
        );

        let oversized_string =
            json!({ "PhysicalDisks": [disk(&"S".repeat(MAX_STRING_BYTES + 1), "U")] }).to_string();
        assert_eq!(
            collect_windows_fixture(&oversized_string),
            Err(CollectorError::OversizedField("windows-disks"))
        );

        let too_many: Vec<Value> = (0..=MAX_ENTITY_COUNT)
            .map(|index| disk(&format!("S-{index}"), &format!("U-{index}")))
            .collect();
        assert_eq!(
            collect_windows_fixture(&json!({ "PhysicalDisks": too_many }).to_string()),
            Err(CollectorError::TooManyEntities)
        );

        let mut too_deep = json!({ "PhysicalDisks": [] });
        for _ in 0..=MAX_JSON_DEPTH {
            too_deep = json!([too_deep]);
        }
        assert_eq!(
            collect_windows_fixture(&too_deep.to_string()),
            Err(CollectorError::TooDeep)
        );

        for invalid_size in [json!(null), json!(0), json!(-1), json!("100")] {
            let mut entry = disk("S-0001", "U-0001");
            entry["Size"] = invalid_size;
            assert_eq!(
                collect_windows_fixture(&json!({ "PhysicalDisks": [entry] }).to_string()),
                Err(CollectorError::InvalidField("Size"))
            );
        }

        // Longer than the contract's 256-byte identity value bound but inside
        // the report's 512-byte string bound.
        let overlong_serial =
            json!({ "PhysicalDisks": [disk(&"S".repeat(257), "U-0001")] }).to_string();
        assert_eq!(
            collect_windows_fixture(&overlong_serial),
            Err(CollectorError::OversizedField("SerialNumber"))
        );
    }

    #[test]
    fn snapshot_metadata_is_validated_like_the_linux_collector() {
        assert!(matches!(
            collect_windows_snapshot(FIXTURE, "not-a-uuid", 1_700_000_000, "host"),
            Err(CollectorError::InvalidSnapshot(_))
        ));
        assert!(matches!(
            collect_windows_snapshot(FIXTURE, "00000000-0000-0000-0000-000000000002", 0, "host"),
            Err(CollectorError::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn fixtures_are_deterministic_and_sanitized() {
        let first = collect_windows_fixture(FIXTURE).unwrap();
        let second = collect_windows_fixture(FIXTURE).unwrap();
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );

        for fixture in [FIXTURE, NO_IDENTITY_FIXTURE] {
            for forbidden in ["\\\\", "Users", "DESKTOP", "C:/", "@", "password"] {
                assert!(
                    !fixture.contains(forbidden),
                    "fixtures must not embed host, path, or credential material: {forbidden}"
                );
            }
        }
    }

    #[test]
    fn the_declared_evidence_command_selects_every_field_the_parser_reads() {
        for field in [
            "PhysicalDisks",
            "DeviceId",
            "FriendlyName",
            "SerialNumber",
            "UniqueId",
            "MediaType",
            "BusType",
            "Size",
            "HealthStatus",
            "OperationalStatus",
            "PhysicalLocation",
        ] {
            assert!(
                WINDOWS_PHYSICAL_DISK_COMMAND.contains(field),
                "declared evidence command must select {field}"
            );
        }
        assert!(WINDOWS_PHYSICAL_DISK_COMMAND.contains("Get-PhysicalDisk"));
        assert!(WINDOWS_PHYSICAL_DISK_COMMAND.contains("-NoProfile"));
        assert!(WINDOWS_PHYSICAL_DISK_COMMAND.contains("-NonInteractive"));
    }
}
