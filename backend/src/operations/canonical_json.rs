use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const MAX_PERSISTED_JSON_BYTES: usize = 64 * 1024;

pub fn to_canonical_string<T: Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    let canonical = canonicalize(value);
    let encoded = serde_json::to_string(&canonical)?;
    if encoded.len() > MAX_PERSISTED_JSON_BYTES {
        bail!(
            "serialized operation payload exceeds {} bytes",
            MAX_PERSISTED_JSON_BYTES
        );
    }
    Ok(encoded)
}

pub fn digest<T: Serialize>(value: &T) -> Result<String> {
    let encoded = to_canonical_string(value)?;
    Ok(hex::encode(Sha256::digest(encoded.as_bytes())))
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let mut entries: Vec<_> = values.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut canonical = serde_json::Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize(value));
            }
            Value::Object(canonical)
        }
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_order_does_not_change_digest() {
        let first = json!({"z": 1, "nested": {"b": 2, "a": 1}});
        let second = json!({"nested": {"a": 1, "b": 2}, "z": 1});
        assert_eq!(digest(&first).unwrap(), digest(&second).unwrap());
    }

    #[test]
    fn oversized_payload_is_rejected() {
        let payload = "x".repeat(MAX_PERSISTED_JSON_BYTES);
        assert!(to_canonical_string(&payload).is_err());
    }
}
