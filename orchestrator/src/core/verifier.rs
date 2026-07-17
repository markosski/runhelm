use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierControlConfig {
    pub max_iterations: u32,
    pub on_exhausted_continue: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerun_from_task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierDecision {
    Continue,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifierAttemptStatus {
    Accepted,
    Rejected,
    ExhaustedAccepted,
    ExhaustedFailed,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifierAttemptMetadata {
    pub status: VerifierAttemptStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<VerifierDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifier_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopFeedbackEntry {
    pub generation: u32,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopExecutionContext {
    pub generation: u32,
    pub max_iterations: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback_history: Vec<LoopFeedbackEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifierExecutionResult {
    pub decision: VerifierDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    pub output: serde_json::Value,
}

pub fn verifier_decision_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["decision"],
        "properties": {
            "decision": {
                "type": "string",
                "enum": ["complete", "continue"]
            },
            "feedback": {
                "type": "string"
            }
        },
        "additionalProperties": true
    })
}
