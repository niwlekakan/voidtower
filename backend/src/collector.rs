use crate::cmdb::contracts::{
    HostObservationV1, IdentityEvidenceV1, InventorySnapshotV1, ObservedEntityV1,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::{path::Path, time::Duration};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

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
    #[error("lsblk field {0} must be a positive integer")]
    InvalidField(&'static str),
    #[error("collected inventory snapshot is invalid: {0}")]
    InvalidSnapshot(String),
}

/// Run the fixed Linux collector command with bounded time and output. A failed
/// command never produces a partial or empty full snapshot.
pub async fn collect_linux_command(
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
) -> Result<InventorySnapshotV1, CollectorError> {
    collect_linux_program_with_timeout(
        Path::new(LSBLK_PROGRAM),
        snapshot_id,
        collected_at,
        host_key,
        LSBLK_TIMEOUT,
    )
    .await
}

pub(crate) async fn collect_linux_program(
    program: &Path,
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
) -> Result<InventorySnapshotV1, CollectorError> {
    collect_linux_program_with_timeout(
        program,
        snapshot_id,
        collected_at,
        host_key,
        LSBLK_TIMEOUT,
    )
    .await
}

async fn collect_linux_program_with_timeout(
    program: &Path,
    snapshot_id: &str,
    collected_at: i64,
    host_key: &str,
    timeout: Duration,
) -> Result<InventorySnapshotV1, CollectorError> {
    use tokio::process::Command;
    let mut child = Command::new(program)
        .args([
            "--json",
            "--bytes",
            "--output",
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
        let stdout_read = read_bounded(stdout, MAX_LSBLK_BYTES);
        let stderr_read = read_bounded(stderr, 4096);
        let (stdout_result, stderr_result) = tokio::join!(stdout_read, stderr_read);
        let out = stdout_result.map_err(|_| CollectorError::CommandFailed)?;
        let err = stderr_result.map_err(|_| CollectorError::CommandFailed)?;
        let status = child
            .wait()
            .await
            .map_err(|_| CollectorError::CommandFailed)?;
        Ok::<_, CollectorError>((status.success(), out, err))
    };
    let (success, output, _diagnostic) = match tokio::time::timeout(timeout, read).await {
        Ok(result) => result?,
        Err(_) => {
            // Dropping a timed-out future with `kill_on_drop` requests a kill,
            // but does not reap the child. Kill and wait explicitly so a retry
            // cannot start while the previous collector process is lingering.
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(CollectorError::Timeout);
        }
    };
    if output.len() > MAX_LSBLK_BYTES || _diagnostic.len() > 4096 {
        return Err(CollectorError::Oversized);
    }
    if !success {
        return Err(CollectorError::CommandFailed);
    }
    let output = String::from_utf8(output).map_err(|_| CollectorError::NonUtf8)?;
    collect_linux_snapshot(&output, snapshot_id, collected_at, host_key)
}

async fn read_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_add(1).saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(retained)
}

/// Parse sanitized lsblk JSON without database knowledge, server identity, or network access.
pub fn collect_linux_fixture(raw: &str) -> Result<InventorySnapshotV1, CollectorError> {
    collect_linux_snapshot(
        raw,
        "00000000-0000-0000-0000-000000000001",
        1_700_000_000,
        "host:fixture",
    )
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
    let snapshot = InventorySnapshotV1 {
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
    };
    snapshot
        .validate()
        .map_err(CollectorError::InvalidSnapshot)?;
    Ok(snapshot)
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
    out.push(ObservedEntityV1 { entity_key: key, entity_type: "physical_disk".into(), identities, attributes: json!({"name":name,"model":model,"serial":serial,"wwn":wwn,"capacity_bytes":positive_integer(v, "size")?,"protocol":bounded_value(v, "tran")?,"rotation":v.get("rota"),"removable":v.get("rm"),"read_only":v.get("ro"),"path":path,"mountpoints":bounded_value(v, "mountpoints")?}), runtime: json!({}), health: json!({}) });
    Ok(())
}
fn positive_integer(v: &Value, field: &'static str) -> Result<u64, CollectorError> {
    v.get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or(CollectorError::InvalidField(field))
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
    use std::io::Cursor;
    const FIXTURE: &str = r#"{"blockdevices":[{"name":"sda","type":"disk","size":100,"model":"Fixture Disk","serial":"SERIAL-001","wwn":"0011223344556677","rota":true,"tran":"sata","rm":false,"ro":false,"path":"/dev/sda","mountpoints":[null]},{"name":"sda1","type":"part"},{"name":"loop0","type":"loop"},{"name":"zram0","type":"ram"}]}"#;
    #[test]
    fn linux_fixture_produces_snapshot_and_filters_ephemeral_devices() {
        let s = collect_linux_fixture(FIXTURE).unwrap();
        assert_eq!((s.schema_version, s.entities.len()), (1, 1));
        assert_eq!(s.entities[0].identities[0].kind, "serial");
        assert!(s.validate().is_ok());
        assert_eq!(s.snapshot_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(s.collected_at, 1_700_000_000);
        assert_eq!(s.entities[0].attributes["protocol"], "sata");
        assert_eq!(s.entities[0].attributes["rotation"], true);
        assert_eq!(s.entities[0].attributes["serial"], "SERIAL-001");
        assert_eq!(s.entities[0].attributes["wwn"], "0011223344556677");
        assert_eq!(s.entities[0].attributes["capacity_bytes"], 100);
        assert!(s.entities[0].attributes.get("size_bytes").is_none());
    }

    #[test]
    fn production_snapshot_rejects_invalid_contract_metadata() {
        assert!(matches!(
            collect_linux_snapshot(FIXTURE, "not-a-uuid", 1_700_000_000, "host"),
            Err(CollectorError::InvalidSnapshot(_))
        ));
        assert!(matches!(
            collect_linux_snapshot(
                FIXTURE,
                "00000000-0000-0000-0000-000000000001",
                0,
                "host"
            ),
            Err(CollectorError::InvalidSnapshot(_))
        ));
    }
    #[test]
    fn physical_disk_capacity_must_be_a_positive_integer() {
        for invalid_size in [json!(null), json!(0), json!(-1), json!("100")] {
            let mut input: Value = serde_json::from_str(FIXTURE).unwrap();
            input["blockdevices"][0]["size"] = invalid_size;
            assert_eq!(
                collect_linux_fixture(&input.to_string()),
                Err(CollectorError::InvalidField("size"))
            );
        }
        let mut missing: Value = serde_json::from_str(FIXTURE).unwrap();
        missing["blockdevices"][0].as_object_mut().unwrap().remove("size");
        assert_eq!(
            collect_linux_fixture(&missing.to_string()),
            Err(CollectorError::InvalidField("size"))
        );
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
            Path::new("/definitely/missing/lsblk"),
            "snapshot",
            0,
            "host",
        )
        .await;
        assert_eq!(missing, Err(CollectorError::CommandFailed));

        let empty = collect_linux_program(Path::new("true"), "snapshot", 0, "host").await;
        assert_eq!(empty, Err(CollectorError::Empty));
    }

    #[cfg(unix)]
    fn executable_fixture(body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::current_dir()
            .unwrap()
            .join(format!(".voidtower-collector-{}.sh", uuid::Uuid::new_v4()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn command_runner_bounds_diagnostics_and_rejects_nonzero_exit() {
        let path = executable_fixture("printf 'diagnostic-fixture' >&2; exit 7");
        let result = collect_linux_program(&path, "snapshot", 1, "host").await;
        std::fs::remove_file(path).unwrap();

        assert_eq!(result, Err(CollectorError::CommandFailed));
        assert!(!CollectorError::CommandFailed.to_string().contains("diagnostic-fixture"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn command_runner_rejects_non_utf8_output() {
        let path = executable_fixture("printf '\\377'");
        let result = collect_linux_program(&path, "snapshot", 1, "host").await;
        std::fs::remove_file(path).unwrap();

        assert_eq!(result, Err(CollectorError::NonUtf8));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn command_runner_rejects_stderr_overflow_without_parsing_stdout() {
        let path = executable_fixture("printf '{\\\"blockdevices\\\":[]}' ; printf '%*s' 4097 '' >&2");
        let result = collect_linux_program(&path, "snapshot", 1, "host").await;
        std::fs::remove_file(path).unwrap();

        assert_eq!(result, Err(CollectorError::Oversized));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn command_runner_timeout_kills_the_child_before_returning() {
        let pid_path = std::env::current_dir()
            .unwrap()
            .join(format!(".voidtower-collector-pid-{}", uuid::Uuid::new_v4()));
        let path = executable_fixture(&format!(
            "printf '%s' \"$$\" > '{}'; exec /bin/sleep 30",
            pid_path.display()
        ));
        let result = collect_linux_program_with_timeout(
            &path,
            "snapshot",
            1,
            "host",
            Duration::from_millis(25),
        )
        .await;
        std::fs::remove_file(path).unwrap();

        assert_eq!(result, Err(CollectorError::Timeout));
        let pid = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(pid) = std::fs::read_to_string(&pid_path) {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let pid = pid.trim();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}/stat")).exists(),
            "timed-out collector must be reaped before returning"
        );
        std::fs::remove_file(pid_path).unwrap();
    }

    #[tokio::test]
    async fn bounded_reader_retains_only_an_overflow_sentinel() {
        let output = read_bounded(
            Cursor::new(vec![b'x'; MAX_LSBLK_BYTES + 1]),
            MAX_LSBLK_BYTES,
        )
        .await
        .unwrap();

        assert_eq!(output.len(), MAX_LSBLK_BYTES + 1);
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
