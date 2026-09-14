use crate::cmdb::contracts::{
    HostObservationV1, IdentityEvidenceV1, InventorySnapshotV1, ObservedEntityV1,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::{path::Path, time::Duration};
use thiserror::Error;

pub const LSBLK_COMMAND: &str = "lsblk --json --bytes --output NAME,KNAME,TYPE,SIZE,MODEL,SERIAL,WWN,ROTA,TRAN,RM,RO,PATH,MOUNTPOINTS";
pub const MAX_LSBLK_BYTES: usize = 256 * 1024;
pub const MAX_ENTITY_COUNT: usize = 128;
pub const MAX_STRING_BYTES: usize = 512;
pub const MAX_JSON_DEPTH: usize = 16;
pub const LSBLK_TIMEOUT: Duration = Duration::from_secs(10);
pub const LSBLK_PROGRAM: &str = "/usr/bin/lsblk";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CollectorError {
    #[error("lsblk output is empty")]
    Empty,
    #[error("lsblk output exceeds {MAX_LSBLK_BYTES} bytes")]
    Oversized,
    #[error("lsblk output is not valid JSON")]
    InvalidJson,
    #[error("lsblk JSON exceeds depth limit")]
    TooDeep,
    #[error("lsblk JSON has no blockdevices array")]
    MissingDevices,
    #[error("lsblk entity count exceeds {MAX_ENTITY_COUNT}")]
    TooManyEntities,
    #[error("lsblk field {0} exceeds {MAX_STRING_BYTES} bytes")]
    OversizedField(&'static str),
    #[error("lsblk physical disk has no stable source identity")]
    MissingIdentity,
    #[error("lsblk physical disk source identities collide")]
    IdentityCollision,
    #[error("lsblk command failed")]
    CommandFailed,
    #[error("lsblk command timed out")]
    Timeout,
    #[error("lsblk output is not UTF-8")]
    NonUtf8,
}

/// Run the fixed Linux collector command with bounded time and output. A failed
/// command never produces a partial or empty full snapshot.
pub async fn collect_linux_command(
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
) -> Result<InventorySnapshotV1, CollectorError> {
    collect_linux_program(Path::new(LSBLK_PROGRAM), snapshot_id, collected_at, host_key).await
}

async fn collect_linux_program(
    program: &Path,
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
) -> Result<InventorySnapshotV1, CollectorError> {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;
    let mut child = Command::new(program)
        .args([
            "--json", "--bytes", "--output",
            "NAME,KNAME,TYPE,SIZE,MODEL,SERIAL,WWN,ROTA,TRAN,RM,RO,PATH,MOUNTPOINTS",
        ])
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| CollectorError::CommandFailed)?;
    let stdout = child.stdout.take().ok_or(CollectorError::CommandFailed)?;
    let stderr = child.stderr.take().ok_or(CollectorError::CommandFailed)?;
    let read = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdout_limited = stdout.take((MAX_LSBLK_BYTES + 1) as u64);
        let mut stderr_limited = stderr.take(4097);
        let stdout_read = stdout_limited.read_to_end(&mut out);
        let stderr_read = stderr_limited.read_to_end(&mut err);
        let (stdout_result, stderr_result) = tokio::join!(stdout_read, stderr_read);
        stdout_result.map_err(|_| CollectorError::CommandFailed)?;
        stderr_result.map_err(|_| CollectorError::CommandFailed)?;
        let status = child.wait().await.map_err(|_| CollectorError::CommandFailed)?;
        Ok::<_, CollectorError>((status.success(), out, err))
    };
    let (success, output, _diagnostic) = tokio::time::timeout(LSBLK_TIMEOUT, read)
        .await.map_err(|_| CollectorError::Timeout)??;
    if !success { return Err(CollectorError::CommandFailed); }
    let output = String::from_utf8(output).map_err(|_| CollectorError::NonUtf8)?;
    collect_linux_snapshot(&output, snapshot_id, collected_at, host_key)
}

/// Parse sanitized lsblk JSON without database knowledge, server identity, or network access.
pub fn collect_linux_fixture(raw: &str) -> Result<InventorySnapshotV1, CollectorError> {
    collect_linux_snapshot(raw, "fixture-snapshot", 0, "host:fixture")
}

pub fn collect_linux_snapshot(
    raw: &str,
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
) -> Result<InventorySnapshotV1, CollectorError> {
    if raw.is_empty() {
        return Err(CollectorError::Empty);
    }
    if raw.len() > MAX_LSBLK_BYTES {
        return Err(CollectorError::Oversized);
    }
    if snapshot_id.len() > MAX_STRING_BYTES {
        return Err(CollectorError::OversizedField("snapshot_id"));
    }
    if host_key.len() > MAX_STRING_BYTES {
        return Err(CollectorError::OversizedField("host_key"));
    }
    let doc: Value = serde_json::from_str(raw).map_err(|_| CollectorError::InvalidJson)?;
    check_depth(&doc, 0)?;
    check_value_strings(&doc, "lsblk", 0)?;
    let devices = doc
        .get("blockdevices")
        .and_then(Value::as_array)
        .ok_or(CollectorError::MissingDevices)?;
    let mut entities = Vec::new();
    let mut source_keys = HashSet::new();
    let mut identity_keys = HashSet::new();
    for device in devices {
        collect_device(device, &mut entities, &mut source_keys, &mut identity_keys)?;
    }
    Ok(InventorySnapshotV1 {
        schema_version: 1,
        snapshot_id: snapshot_id.into(),
        collector_version: "linux-collector-v1".into(),
        platform: "linux".into(),
        collected_at,
        host: HostObservationV1 {
            entity_key: host_key.into(),
            identities: Vec::new(),
            attributes: json!({"source":"linux"}),
            runtime: json!({"collection":"complete"}),
        },
        entities,
    })
}
fn collect_device(
    v: &Value,
    out: &mut Vec<ObservedEntityV1>,
    source_keys: &mut HashSet<String>,
    identity_keys: &mut HashSet<String>,
) -> Result<(), CollectorError> {
    if let Some(children) = v.get("children").and_then(Value::as_array) {
        for child in children {
            collect_device(child, out, source_keys, identity_keys)?;
        }
    }
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default();
    if matches!(kind, "loop" | "ram" | "part") || kind.starts_with("dm-") || kind != "disk" {
        return Ok(());
    }
    if out.len() >= MAX_ENTITY_COUNT {
        return Err(CollectorError::TooManyEntities);
    }
    let name = bounded(v, "name")?;
    let model = bounded(v, "model")?;
    let serial = bounded(v, "serial")?;
    let wwn = bounded(v, "wwn")?;
    let path = bounded(v, "path")?;
    let mut identities = Vec::new();
    if let Some(x) = serial.as_ref().filter(|x| !x.is_empty()) {
        identities.push(IdentityEvidenceV1 {
            kind: "serial".into(),
            value: x.clone(),
        });
    }
    if let Some(x) = wwn.as_ref().filter(|x| !x.is_empty()) {
        identities.push(IdentityEvidenceV1 {
            kind: "wwn".into(),
            value: x.clone(),
        });
    }
    for identity in &identities {
        if !identity_keys.insert(format!("{}:{}", identity.kind, identity.value)) {
            return Err(CollectorError::IdentityCollision);
        }
    }
    let key = format!(
        "physical-disk:{}",
        serial
            .as_deref()
            .filter(|x| !x.is_empty())
            .or(wwn.as_deref().filter(|x| !x.is_empty()))
            .ok_or(CollectorError::MissingIdentity)?
    );
    if !source_keys.insert(key.clone()) {
        return Err(CollectorError::IdentityCollision);
    }
    out.push(ObservedEntityV1 { entity_key: key, entity_type: "physical_disk".into(), identities, attributes: json!({"name":name,"model":model,"size_bytes":v.get("size"),"transport":bounded_value(v, "tran")?,"removable":v.get("rm"),"read_only":v.get("ro"),"path":path,"mountpoints":bounded_value(v, "mountpoints")?}), runtime: json!({"rotation":v.get("rota")}), health: json!({}) });
    Ok(())
}
fn bounded(v: &Value, field: &'static str) -> Result<Option<String>, CollectorError> {
    let Some(x) = v.get(field).and_then(Value::as_str) else {
        return Ok(None);
    };
    if x.len() > MAX_STRING_BYTES {
        return Err(CollectorError::OversizedField(field));
    }
    Ok(Some(x.into()))
}
fn bounded_value(v: &Value, field: &'static str) -> Result<Value, CollectorError> {
    let value = v.get(field).cloned().unwrap_or(Value::Null);
    check_value_strings(&value, field, 0)?;
    Ok(value)
}
fn check_value_strings(v: &Value, field: &'static str, depth: usize) -> Result<(), CollectorError> {
    if depth > 4 {
        return Err(CollectorError::TooDeep);
    }
    match v {
        Value::String(x) if x.len() > MAX_STRING_BYTES => {
            Err(CollectorError::OversizedField(field))
        }
        Value::Array(xs) => {
            if xs.len() > MAX_ENTITY_COUNT {
                return Err(CollectorError::TooManyEntities);
            }
            xs.iter()
                .try_for_each(|x| check_value_strings(x, field, depth + 1))
        }
        Value::Object(xs) => xs
            .values()
            .try_for_each(|x| check_value_strings(x, field, depth + 1)),
        _ => Ok(()),
    }
}
fn check_depth(v: &Value, depth: usize) -> Result<(), CollectorError> {
    if depth > MAX_JSON_DEPTH {
        return Err(CollectorError::TooDeep);
    }
    match v {
        Value::Array(xs) => xs.iter().try_for_each(|x| check_depth(x, depth + 1)),
        Value::Object(xs) => xs.values().try_for_each(|x| check_depth(x, depth + 1)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = r#"{"blockdevices":[{"name":"sda","type":"disk","size":100,"model":"Fixture Disk","serial":"SERIAL-001","wwn":"wwn-001","rota":true,"tran":"sata","rm":false,"ro":false,"path":"/dev/sda","mountpoints":[null]},{"name":"sda1","type":"part"},{"name":"loop0","type":"loop"},{"name":"zram0","type":"ram"}]}"#;
    #[test]
    fn linux_fixture_produces_snapshot_and_filters_ephemeral_devices() {
        let s = collect_linux_fixture(FIXTURE).unwrap();
        assert_eq!((s.schema_version, s.entities.len()), (1, 1));
        assert_eq!(s.entities[0].identities[0].kind, "serial");
    }
    #[test]
    fn malformed_oversized_and_missing_input_fail_closed() {
        assert_eq!(collect_linux_fixture("{"), Err(CollectorError::InvalidJson));
        assert_eq!(
            collect_linux_fixture(&"x".repeat(MAX_LSBLK_BYTES + 1)),
            Err(CollectorError::Oversized)
        );
        assert_eq!(
            collect_linux_fixture("{}"),
            Err(CollectorError::MissingDevices)
        );
    }
    #[tokio::test]
    async fn command_runner_fails_closed_when_program_is_missing_or_empty() {
        let missing = collect_linux_program(
            Path::new("/definitely/missing/lsblk"), "snapshot", 0, "host",
        ).await;
        assert_eq!(missing, Err(CollectorError::CommandFailed));

        let empty = collect_linux_program(Path::new("true"), "snapshot", 0, "host").await;
        assert_eq!(empty, Err(CollectorError::Empty));
    }

    #[test]
    fn oversized_field_and_deep_json_fail_closed() {
        let x = format!(
            r#"{{"blockdevices":[{{"type":"disk","model":"{}"}}]}}"#,
            "x".repeat(MAX_STRING_BYTES + 1)
        );
        assert_eq!(
            collect_linux_fixture(&x),
            Err(CollectorError::OversizedField("lsblk"))
        );
        let mut v = json!({"blockdevices":[]});
        for _ in 0..=MAX_JSON_DEPTH {
            v = json!([v]);
        }
        assert_eq!(
            collect_linux_fixture(&v.to_string()),
            Err(CollectorError::TooDeep)
        );
    }
}
