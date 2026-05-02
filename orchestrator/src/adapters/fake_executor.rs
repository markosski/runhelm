use crate::core::models::TaskDef;
use crate::ports::executor::ExecutorPort;
use async_trait::async_trait;
use serde_json::Value;

/// A deterministic, in-process executor that generates schema-conformant default
/// output from a `TaskDef`'s `output_schema`. Used in unit tests and dry-runs.
pub struct FakeExecutor;

impl FakeExecutor {
    pub fn new() -> Self {
        FakeExecutor
    }
}

impl Default for FakeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Recursively walk a JSON Schema and produce a default value that conforms to it.
///
/// Rules:
/// - `"type": "object"` → `{}` unless `required` fields are listed, in which case
///   each required field is generated from its entry in `properties`.
/// - `"type": "string"` → `""`
/// - `"type": "integer"` | `"number"` → `0`
/// - `"type": "boolean"` → `false`
/// - `"type": "array"` → `[]`
/// - `"type": "null"` → `null`
/// - No `type` field, or unsupported constructs (`oneOf`, `anyOf`, `$ref`) → `{}`
fn schema_default(schema: &Value) -> Value {
    let type_str = schema.get("type").and_then(Value::as_str);

    match type_str {
        Some("object") => {
            let required = schema
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            if required.is_empty() {
                return Value::Object(serde_json::Map::new());
            }

            let properties = schema.get("properties");
            let mut map = serde_json::Map::new();

            for field in &required {
                if let Some(field_name) = field.as_str() {
                    let field_schema = properties
                        .and_then(|p| p.get(field_name))
                        .cloned()
                        .unwrap_or(Value::Object(serde_json::Map::new()));
                    map.insert(field_name.to_string(), schema_default(&field_schema));
                }
            }

            Value::Object(map)
        }
        Some("string") => Value::String(String::new()),
        Some("integer") | Some("number") => Value::Number(0.into()),
        Some("boolean") => Value::Bool(false),
        Some("array") => Value::Array(vec![]),
        Some("null") => Value::Null,
        // No type or unsupported constructs (oneOf, anyOf, $ref, …) → graceful fallback
        _ => Value::Object(serde_json::Map::new()),
    }
}

#[async_trait]
impl ExecutorPort for FakeExecutor {
    async fn execute(
        &self,
        task: &TaskDef,
        _inputs: &[Value],
    ) -> anyhow::Result<Value> {
        Ok(match &task.output_schema {
            Some(schema) => schema_default(schema),
            None => Value::Null,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fake() -> FakeExecutor {
        FakeExecutor::new()
    }

    fn task_with_schema(schema: Value) -> TaskDef {
        TaskDef {
            id: "test-task".to_string(),
            kind: crate::core::models::TaskTypeDef::ApiCall {
                url: "http://example.com".to_string(),
                method: "GET".to_string(),
            },
            input_schemas: vec![],
            output_schema: Some(schema),
            expected_side_effects: vec![],
            required_credentials: vec![],
        }
    }

    #[tokio::test]
    async fn test_empty_object_schema() {
        let task = task_with_schema(json!({"type": "object"}));
        let result = fake().execute(&task, &[]).await.unwrap();
        assert_eq!(result, json!({}));
    }

    #[tokio::test]
    async fn test_object_with_required_fields() {
        let task = task_with_schema(json!({
            "type": "object",
            "required": ["name", "count"],
            "properties": {
                "name": {"type": "string"},
                "count": {"type": "integer"}
            }
        }));
        let result = fake().execute(&task, &[]).await.unwrap();
        assert_eq!(result, json!({"name": "", "count": 0}));
    }

    #[tokio::test]
    async fn test_string_schema() {
        let task = task_with_schema(json!({"type": "string"}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!(""));
    }

    #[tokio::test]
    async fn test_integer_schema() {
        let task = task_with_schema(json!({"type": "integer"}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!(0));
    }

    #[tokio::test]
    async fn test_number_schema() {
        let task = task_with_schema(json!({"type": "number"}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!(0));
    }

    #[tokio::test]
    async fn test_boolean_schema() {
        let task = task_with_schema(json!({"type": "boolean"}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!(false));
    }

    #[tokio::test]
    async fn test_array_schema() {
        let task = task_with_schema(json!({"type": "array"}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!([]));
    }

    #[tokio::test]
    async fn test_null_schema() {
        let task = task_with_schema(json!({"type": "null"}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), Value::Null);
    }

    #[tokio::test]
    async fn test_no_type_schema() {
        let task = task_with_schema(json!({}));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!({}));
    }

    #[tokio::test]
    async fn test_one_of_fallback() {
        let task = task_with_schema(json!({
            "oneOf": [{"type": "string"}, {"type": "integer"}]
        }));
        assert_eq!(fake().execute(&task, &[]).await.unwrap(), json!({}));
    }

    #[tokio::test]
    async fn test_inputs_do_not_affect_output() {
        let task = task_with_schema(json!({"type": "string"}));
        let out1 = fake().execute(&task, &[]).await.unwrap();
        let out2 = fake().execute(&task, &[json!("anything"), json!(42)]).await.unwrap();
        assert_eq!(out1, out2);
    }

    #[tokio::test]
    async fn test_output_valid_against_strict_schema() {
        let schema = json!({
            "type": "object",
            "required": ["label", "score"],
            "properties": {
                "label": {"type": "string"},
                "score": {"type": "number"}
            },
            "additionalProperties": false
        });
        let task = task_with_schema(schema.clone());
        let output = fake().execute(&task, &[]).await.unwrap();

        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&output), "output did not satisfy schema: {output}");
    }
}
