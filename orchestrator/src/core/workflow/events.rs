use crate::core::models::{
    TaskInputMapping, TaskInstance, TaskSatisfactionStatus, TaskStatus, VerifierAttemptMetadata,
};
use crate::core::workflow::models::{
    VerifierFeedbackEntry, VerifierGenerationState, VerifierStateStatus, WorkflowInstance,
    WorkflowStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowInstanceEvent {
    /// Initializes a new workflow instance snapshot.
    WorkflowCreated { instance: WorkflowInstance },
    /// Changes the overall workflow instance status.
    WorkflowStatusChanged { status: WorkflowStatus },
    /// Resets in-flight running workflow or task state after orchestrator restart.
    StartupRecoveryApplied,
    /// Adds a concrete task attempt to the workflow instance.
    TaskMaterialized {
        task_attempt_id: String,
        task: TaskInstance,
    },
    /// Changes the status of an existing task attempt.
    TaskStatusChanged {
        task_attempt_id: String,
        status: TaskStatus,
    },
    /// Replaces the input data for an existing task attempt.
    TaskInputDataSet {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
    },
    /// Replaces the upstream input mapping for an existing task attempt.
    TaskInputMappingSet {
        task_attempt_id: String,
        input_mapping: Vec<TaskInputMapping>,
    },
    /// Records or clears output data for an existing task attempt.
    TaskOutputRecorded {
        task_attempt_id: String,
        output_data: Option<serde_json::Value>,
    },
    /// Changes whether a task attempt is pending, satisfied, or unsatisfied.
    TaskSatisfactionChanged {
        task_attempt_id: String,
        satisfaction_status: TaskSatisfactionStatus,
    },
    /// Sets or clears verifier metadata on a task attempt.
    TaskVerifierMetadataSet {
        task_attempt_id: String,
        verifier_metadata: Option<VerifierAttemptMetadata>,
    },
    /// Creates or replaces verifier generation state for a verifier task.
    VerifierStateUpserted {
        verifier_task_id: String,
        state: VerifierGenerationState,
    },
    /// Appends verifier feedback history for a verifier task.
    VerifierFeedbackRecorded {
        verifier_task_id: String,
        feedback: VerifierFeedbackEntry,
    },
    /// Updates verifier state status, selected generation, and exit reason.
    VerifierStateStatusChanged {
        verifier_task_id: String,
        status: VerifierStateStatus,
        selected_generation: Option<u32>,
        exit_reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEventRecord {
    pub created_time: u64,
    pub event: WorkflowInstanceEvent,
}

pub fn apply_workflow_instance_event(
    instance: &mut WorkflowInstance,
    event: &WorkflowInstanceEvent,
) -> anyhow::Result<()> {
    match event {
        WorkflowInstanceEvent::WorkflowCreated { .. } => {
            anyhow::bail!("workflow_created can only initialize an empty workflow snapshot")
        }
        WorkflowInstanceEvent::WorkflowStatusChanged { status } => {
            instance.status = status.clone();
        }
        WorkflowInstanceEvent::StartupRecoveryApplied => {
            if instance.status == WorkflowStatus::Running {
                instance.status = WorkflowStatus::Pending;
            }
            for task in instance.tasks.values_mut() {
                if task.status == TaskStatus::Running {
                    task.status = TaskStatus::Pending;
                }
            }
        }
        WorkflowInstanceEvent::TaskMaterialized {
            task_attempt_id,
            task,
        } => {
            if instance.tasks.contains_key(task_attempt_id) {
                anyhow::bail!("task attempt {task_attempt_id} already exists");
            }
            instance.tasks.insert(task_attempt_id.clone(), task.clone());
        }
        WorkflowInstanceEvent::TaskStatusChanged {
            task_attempt_id,
            status,
        } => {
            task_mut(instance, task_attempt_id)?.status = status.clone();
        }
        WorkflowInstanceEvent::TaskInputDataSet {
            task_attempt_id,
            input_data,
        } => {
            task_mut(instance, task_attempt_id)?.input_data = input_data.clone();
        }
        WorkflowInstanceEvent::TaskInputMappingSet {
            task_attempt_id,
            input_mapping,
        } => {
            task_mut(instance, task_attempt_id)?.input_mapping = input_mapping.clone();
        }
        WorkflowInstanceEvent::TaskOutputRecorded {
            task_attempt_id,
            output_data,
        } => {
            task_mut(instance, task_attempt_id)?.output_data = output_data.clone();
        }
        WorkflowInstanceEvent::TaskSatisfactionChanged {
            task_attempt_id,
            satisfaction_status,
        } => {
            task_mut(instance, task_attempt_id)?.satisfaction_status = satisfaction_status.clone();
        }
        WorkflowInstanceEvent::TaskVerifierMetadataSet {
            task_attempt_id,
            verifier_metadata,
        } => {
            task_mut(instance, task_attempt_id)?.verifier_metadata = verifier_metadata.clone();
        }
        WorkflowInstanceEvent::VerifierStateUpserted {
            verifier_task_id,
            state,
        } => {
            instance
                .verifier_states
                .insert(verifier_task_id.clone(), state.clone());
        }
        WorkflowInstanceEvent::VerifierFeedbackRecorded {
            verifier_task_id,
            feedback,
        } => {
            let state = verifier_state_mut(instance, verifier_task_id)?;
            state.feedback_history.push(feedback.clone());
        }
        WorkflowInstanceEvent::VerifierStateStatusChanged {
            verifier_task_id,
            status,
            selected_generation,
            exit_reason,
        } => {
            let state = verifier_state_mut(instance, verifier_task_id)?;
            state.status = status.clone();
            state.selected_generation = *selected_generation;
            state.exit_reason = exit_reason.clone();
        }
    }

    Ok(())
}

pub fn reduce_workflow_instance_events(
    current: Option<WorkflowInstance>,
    events: &[WorkflowInstanceEvent],
) -> anyhow::Result<WorkflowInstance> {
    let mut iter = events.iter();
    let mut instance = match current {
        Some(instance) => instance,
        None => match iter.next() {
            Some(WorkflowInstanceEvent::WorkflowCreated { instance }) => instance.clone(),
            Some(_) => anyhow::bail!("first event for a missing workflow snapshot must create it"),
            None => anyhow::bail!("event batch must not be empty"),
        },
    };

    for event in iter {
        apply_workflow_instance_event(&mut instance, event)?;
    }

    Ok(instance)
}

fn task_mut<'a>(
    instance: &'a mut WorkflowInstance,
    task_attempt_id: &str,
) -> anyhow::Result<&'a mut TaskInstance> {
    instance
        .tasks
        .get_mut(task_attempt_id)
        .ok_or_else(|| anyhow::anyhow!("task attempt {task_attempt_id} not found"))
}

fn verifier_state_mut<'a>(
    instance: &'a mut WorkflowInstance,
    verifier_task_id: &str,
) -> anyhow::Result<&'a mut VerifierGenerationState> {
    instance
        .verifier_states
        .get_mut(verifier_task_id)
        .ok_or_else(|| anyhow::anyhow!("verifier state {verifier_task_id} not found"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::TaskSatisfactionStatus;
    use std::collections::HashMap;

    fn instance() -> WorkflowInstance {
        WorkflowInstance {
            id: "wf-1".to_string(),
            workflow_def_id: "wf".to_string(),
            status: WorkflowStatus::Pending,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    fn task() -> TaskInstance {
        TaskInstance {
            task_def_id: "task-a".to_string(),
            status: TaskStatus::Pending,
            satisfaction_status: TaskSatisfactionStatus::Pending,
            input_data: vec![],
            input_mapping: vec![],
            output_data: None,
            generation_index: 1,
            verifier_metadata: None,
        }
    }

    #[test]
    fn reducer_initializes_workflow_from_create_event() {
        let reduced = reduce_workflow_instance_events(
            None,
            &[WorkflowInstanceEvent::WorkflowCreated {
                instance: instance(),
            }],
        )
        .unwrap();

        assert_eq!(reduced.id, "wf-1");
        assert_eq!(reduced.status, WorkflowStatus::Pending);
    }

    #[test]
    fn reducer_applies_ordered_task_events() {
        let instance = reduce_workflow_instance_events(
            Some(instance()),
            &[
                WorkflowInstanceEvent::TaskMaterialized {
                    task_attempt_id: "task-a[1]".to_string(),
                    task: task(),
                },
                WorkflowInstanceEvent::TaskStatusChanged {
                    task_attempt_id: "task-a[1]".to_string(),
                    status: TaskStatus::Running,
                },
                WorkflowInstanceEvent::TaskStatusChanged {
                    task_attempt_id: "task-a[1]".to_string(),
                    status: TaskStatus::Completed,
                },
                WorkflowInstanceEvent::TaskStatusChanged {
                    task_attempt_id: "task-a[1]".to_string(),
                    status: TaskStatus::Failed,
                },
            ],
        )
        .unwrap();

        assert_eq!(instance.tasks["task-a[1]"].status, TaskStatus::Failed);
    }

    #[test]
    fn reducer_rejects_missing_task_updates() {
        let mut instance = instance();
        let result = apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::TaskStatusChanged {
                task_attempt_id: "task-a[1]".to_string(),
                status: TaskStatus::Running,
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn reducer_applies_verifier_events() {
        let instance = reduce_workflow_instance_events(
            Some(instance()),
            &[
                WorkflowInstanceEvent::VerifierStateUpserted {
                    verifier_task_id: "verify".to_string(),
                    state: VerifierGenerationState {
                        verifier_task_id: "verify".to_string(),
                        rerun_start_task_id: "task-a".to_string(),
                        latest_generation: 1,
                        selected_generation: None,
                        feedback_history: vec![],
                        status: VerifierStateStatus::Running,
                        exit_reason: None,
                    },
                },
                WorkflowInstanceEvent::VerifierFeedbackRecorded {
                    verifier_task_id: "verify".to_string(),
                    feedback: VerifierFeedbackEntry {
                        generation_index: 1,
                        feedback: "retry".to_string(),
                        verifier_output: serde_json::json!({"decision": "continue"}),
                    },
                },
                WorkflowInstanceEvent::VerifierStateStatusChanged {
                    verifier_task_id: "verify".to_string(),
                    status: VerifierStateStatus::Accepted,
                    selected_generation: Some(1),
                    exit_reason: None,
                },
            ],
        )
        .unwrap();

        let state = &instance.verifier_states["verify"];
        assert_eq!(state.status, VerifierStateStatus::Accepted);
        assert_eq!(state.feedback_history.len(), 1);
        assert_eq!(state.selected_generation, Some(1));
    }

    #[test]
    fn reducer_applies_startup_recovery() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Running;
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Running,
                ..task()
            },
        );

        apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::StartupRecoveryApplied,
        )
        .unwrap();

        assert_eq!(instance.status, WorkflowStatus::Pending);
        assert_eq!(instance.tasks["task-a[1]"].status, TaskStatus::Pending);
    }
}
