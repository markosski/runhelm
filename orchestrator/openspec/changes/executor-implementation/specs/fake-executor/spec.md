# Capability: fake-executor

## Purpose
Defines the behavior of `FakeExecutor` — a deterministic, in-process implementation of `ExecutorPort` that generates schema-conformant default output for any well-formed JSON Schema. It is used in unit tests and dry-run scenarios where real task execution is neither desired nor available.

## Requirements

### Requirement: Schema-Conformant Default Output
The `FakeExecutor` SHALL produce a `serde_json::Value` that satisfies the `output_schema` of the given `TaskDef` by recursively applying typed default values.

#### Scenario: Object schema with no required fields
- **WHEN** `output_schema` is `{"type": "object"}` with no `required` array
- **THEN** `FakeExecutor` SHALL return `{}`

#### Scenario: Object schema with required fields
- **WHEN** `output_schema` is `{"type": "object", "required": ["name", "count"], "properties": {"name": {"type": "string"}, "count": {"type": "integer"}}}`
- **THEN** `FakeExecutor` SHALL return `{"name": "", "count": 0}`

#### Scenario: Primitive schemas
- **WHEN** `output_schema` is `{"type": "string"}`
- **THEN** `FakeExecutor` SHALL return `""`
- **WHEN** `output_schema` is `{"type": "integer"}` or `{"type": "number"}`
- **THEN** `FakeExecutor` SHALL return `0`
- **WHEN** `output_schema` is `{"type": "boolean"}`
- **THEN** `FakeExecutor` SHALL return `false`
- **WHEN** `output_schema` is `{"type": "array"}`
- **THEN** `FakeExecutor` SHALL return `[]`
- **WHEN** `output_schema` is `{"type": "null"}`
- **THEN** `FakeExecutor` SHALL return `null`

#### Scenario: Schema with no type field
- **WHEN** `output_schema` has no `type` field (e.g., `{}`)
- **THEN** `FakeExecutor` SHALL return `{}`

### Requirement: Unsupported Schema Constructs
For JSON Schema constructs not explicitly handled (`oneOf`, `anyOf`, `$ref`, etc.), `FakeExecutor` SHALL return `{}` rather than failing.

#### Scenario: Complex schema graceful fallback
- **WHEN** `output_schema` contains `oneOf`, `anyOf`, or `$ref` constructs
- **THEN** `FakeExecutor` SHALL return `{}` and SHALL NOT return an error

### Requirement: Infallibility
`FakeExecutor::execute` SHALL always return `Ok(...)` and SHALL never return `Err(...)`. It has no external dependencies and cannot fail.

#### Scenario: Always succeeds
- **WHEN** `FakeExecutor::execute` is called with any `TaskDef` and any input slice
- **THEN** it SHALL return `Ok(serde_json::Value)` without error

### Requirement: Input Agnosticism
`FakeExecutor` SHALL ignore the provided input values entirely. Its output is derived solely from `TaskDef.output_schema`.

#### Scenario: Inputs do not affect output
- **WHEN** `FakeExecutor::execute` is called with different input arrays but the same `TaskDef`
- **THEN** it SHALL return the same output value each time
