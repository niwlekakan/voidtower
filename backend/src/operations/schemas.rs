use crate::api::mcp::action_registry::{ActionExecution, ActionMetadata};
use crate::operations::canonical_json;
use serde_json::Value;

pub const MAX_ACTION_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_ACTION_INPUT_DEPTH: usize = 8;
pub const MAX_ACTION_INPUT_PROPERTIES: usize = 64;
pub const MAX_ACTION_INPUT_ITEMS: usize = 64;
pub const MAX_ACTION_INPUT_STRING_CHARS: usize = 4 * 1024;
pub const MAX_ACTION_INPUT_KEY_CHARS: usize = 128;
const MAX_ACTION_RESULT_STRING_CHARS: usize = MAX_ACTION_INPUT_STRING_CHARS + 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSchema {
    pub input_id: &'static str,
    pub result_id: &'static str,
}

pub fn schema_for_action(action: &ActionMetadata) -> Option<ActionSchema> {
    let registered = crate::api::mcp::action_registry::action(action.name)?;
    if action != registered {
        return None;
    }
    if action.execution != ActionExecution::DurableJob {
        return None;
    }
    let input_id = action.input_schema_id?;
    let result_id = action.result_schema_id?;
    let expected_input = format!("{}.input.v1", action.name);
    let expected_result = format!("{}.result.v1", action.name);
    (input_id == expected_input && result_id == expected_result).then_some(ActionSchema {
        input_id,
        result_id,
    })
}

pub fn validate_input(action: &ActionMetadata, input: &Value) -> Result<(), &'static str> {
    schema_for_action(action).ok_or("schema_unregistered")?;
    if !input.is_object() {
        return Err("object_required");
    }
    validate_value(input, 0)?;
    canonical_json::to_canonical_string_with_limit(input, MAX_ACTION_INPUT_BYTES)
        .map(|_| ())
        .map_err(|_| "input_too_large")
}

pub fn validate_result(action: &ActionMetadata, result: &Value) -> Result<(), &'static str> {
    schema_for_action(action).ok_or("schema_unregistered")?;
    if result.is_null() {
        return Err("result_required");
    }
    validate_value_with_string_limit(result, 0, MAX_ACTION_RESULT_STRING_CHARS)?;
    canonical_json::to_canonical_string_with_limit(result, MAX_ACTION_INPUT_BYTES)
        .map(|_| ())
        .map_err(|_| "result_too_large")
}

fn validate_value(value: &Value, depth: usize) -> Result<(), &'static str> {
    validate_value_with_string_limit(value, depth, MAX_ACTION_INPUT_STRING_CHARS)
}

fn validate_value_with_string_limit(
    value: &Value,
    depth: usize,
    max_string_chars: usize,
) -> Result<(), &'static str> {
    if depth > MAX_ACTION_INPUT_DEPTH {
        return Err("depth_exceeded");
    }
    match value {
        Value::Object(values) => {
            if values.len() > MAX_ACTION_INPUT_PROPERTIES {
                return Err("too_many_properties");
            }
            for (key, value) in values {
                if key.chars().count() > MAX_ACTION_INPUT_KEY_CHARS {
                    return Err("key_too_long");
                }
                validate_value_with_string_limit(value, depth + 1, max_string_chars)?;
            }
        }
        Value::Array(values) => {
            if values.len() > MAX_ACTION_INPUT_ITEMS {
                return Err("too_many_items");
            }
            for value in values {
                validate_value_with_string_limit(value, depth + 1, max_string_chars)?;
            }
        }
        Value::String(value) if value.chars().count() > max_string_chars => {
            return Err("string_too_long");
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mcp::action_registry;
    use serde_json::json;

    #[test]
    fn every_durable_action_resolves_its_registered_schema_ids() {
        for action in action_registry::ACTIONS
            .iter()
            .filter(|action| action.execution == action_registry::ActionExecution::DurableJob)
        {
            let schema = schema_for_action(action).expect("durable action schema must resolve");
            assert_eq!(schema.input_id, action.input_schema_id.unwrap());
            assert_eq!(schema.result_id, action.result_schema_id.unwrap());
        }
    }

    #[test]
    fn action_input_is_a_bounded_object() {
        let action = action_registry::action("container.start").unwrap();
        assert!(validate_input(action, &json!({"requested": true})).is_ok());
        assert_eq!(validate_input(action, &json!(null)), Err("object_required"));
        assert_eq!(validate_input(action, &json!([])), Err("object_required"));
    }

    #[test]
    fn action_input_rejects_excessive_shape_before_adapter_execution() {
        let action = action_registry::action("container.start").unwrap();
        let too_many = (0..65)
            .map(|index| (format!("field_{index}"), json!(true)))
            .collect();
        assert_eq!(
            validate_input(action, &Value::Object(too_many)),
            Err("too_many_properties")
        );
        assert_eq!(
            validate_input(action, &json!({"value": "x".repeat(4097)})),
            Err("string_too_long")
        );
    }

    #[test]
    fn action_input_rejects_excessive_nesting() {
        let action = action_registry::action("container.start").unwrap();
        let mut value = json!(true);
        for _ in 0..=MAX_ACTION_INPUT_DEPTH {
            value = json!({"nested": value});
        }
        assert_eq!(validate_input(action, &value), Err("depth_exceeded"));
    }

    #[test]
    fn schema_identity_cannot_be_self_declared_by_unregistered_metadata() {
        let registered = action_registry::action("container.start").unwrap();
        let forged = ActionMetadata {
            input_schema_id: Some("container.start.forged.input.v1"),
            result_schema_id: registered.result_schema_id,
            ..*registered
        };
        assert_eq!(schema_for_action(&forged), None);
        assert_eq!(
            validate_input(&forged, &json!({})),
            Err("schema_unregistered")
        );
    }

    #[test]
    fn action_result_uses_the_same_bounded_registered_schema() {
        let action = action_registry::action("container.start").unwrap();
        assert!(validate_result(action, &json!({"changed": true})).is_ok());
        assert_eq!(
            validate_result(action, &Value::Null),
            Err("result_required")
        );
        let oversized = (0..MAX_ACTION_INPUT_ITEMS)
            .map(|_| Value::String("x".repeat(1024)))
            .collect();
        assert_eq!(
            validate_result(action, &Value::Array(oversized)),
            Err("result_too_large")
        );
    }
}
