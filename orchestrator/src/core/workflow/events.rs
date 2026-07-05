use crate::core::models::{
    TaskInputMapping, TaskInstance, TaskSatisfactionStatus, TaskStatus, VerifierAttemptMetadata,
};
use crate::core::workflow::models::{
    VerifierFeedbackEntry, VerifierGenerationState, VerifierStateStatus, WorkerHostId,
    WorkflowInstance, WorkflowStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowInstanceEvent {
    /// Initializes a new workflow instance snapshot.
    WorkflowCreated { instance: WorkflowInstance },
    /// Starts execution for a workflow instance.
    WorkflowRunStarted,
    /// Completes execution for a workflow instance.
    WorkflowRunCompleted,
    /// Fails execution for a workflow instance.
    WorkflowRunFailed,
    /// Pauses workflow execution.
    WorkflowPaused,
    /// Resumes workflow execution.
    WorkflowResumed,
    /// Adds concrete task attempts and verifier state to the workflow instance.
    TaskAttemptsMaterialized {
        tasks: Vec<TaskInstance>,
        verifier_states: Vec<VerifierGenerationState>,
    },
    /// Starts a concrete task attempt.
    TaskAttemptStarted { task_attempt_id: String },
    /// Records a task failure caused by invalid resolved input.
    TaskInputValidationFailed {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
    },
    /// Records a task waiting for human input.
    TaskInputNeeded {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
        input_request: String,
    },
    /// Records a failed task attempt.
    TaskAttemptFailed {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
        mark_unsatisfied: bool,
    },
    /// Records a successful task attempt and any verifier outcome it produced.
    TaskAttemptSucceeded {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
        output_data: Option<serde_json::Value>,
        satisfaction_status: Option<TaskSatisfactionStatus>,
        verifier_outcome: Option<TaskVerifierOutcome>,
    },
    /// Records accepted human input and the continuation attempt it materialized.
    HumanInputSubmitted {
        task_attempt_id: String,
        submitted_input: serde_json::Value,
    },
    /// Restarts a failed task attempt on the current pinned host.
    TaskRetryStarted { task_attempt_id: String },
    /// Restarts a failed task attempt on a target host.
    TaskForceRetryStarted {
        task_attempt_id: String,
        target_host_id: WorkerHostId,
    },
    /// Reopens in-flight work after orchestrator startup.
    StartupRecoveryApplied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskVerifierOutcome {
    Accepted {
        verifier_metadata: VerifierAttemptMetadata,
        satisfied_task_attempt_ids: Vec<String>,
    },
    RejectedWithRerun {
        verifier_metadata: VerifierAttemptMetadata,
        unsatisfied_task_attempt_ids: Vec<String>,
        materialized_tasks: Vec<TaskInstance>,
    },
    Invalid {
        verifier_metadata: VerifierAttemptMetadata,
    },
    Failed {
        status: VerifierStateStatus,
        selected_generation: Option<u32>,
        exit_reason: String,
        verifier_metadata: VerifierAttemptMetadata,
        unsatisfied_task_attempt_ids: Vec<String>,
    },
    ExhaustedAccepted {
        verifier_metadata: VerifierAttemptMetadata,
        satisfied_task_attempt_ids: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub enum WorkflowInstanceCommand {
    CreateWorkflow {
        instance: WorkflowInstance,
    },
    StartWorkflowRun,
    CompleteWorkflowRun,
    FailWorkflowRun,
    PauseWorkflow,
    ResumeWorkflow,
    MaterializeTaskAttempts {
        tasks: Vec<TaskInstance>,
        verifier_states: Vec<VerifierGenerationState>,
    },
    StartTaskAttempt {
        task_attempt_id: String,
    },
    RecordTaskInputValidationFailed {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
    },
    RecordTaskInputNeeded {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
        input_request: String,
    },
    RecordTaskFailed {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
        mark_unsatisfied: bool,
    },
    RecordTaskSucceeded {
        task_attempt_id: String,
        input_data: Vec<serde_json::Value>,
        input_mapping: Vec<TaskInputMapping>,
        output_data: Option<serde_json::Value>,
        satisfaction_status: Option<TaskSatisfactionStatus>,
        verifier_outcome: Option<TaskVerifierOutcome>,
    },
    SubmitHumanInput {
        task_attempt_id: String,
        submitted_input: serde_json::Value,
    },
    RetryTask {
        task_attempt_id: String,
    },
    ForceRetryTask {
        task_attempt_id: String,
        target_host_id: WorkerHostId,
    },
    ApplyStartupRecovery,
}

#[derive(Debug)]
pub struct WorkflowTransition {
    pub instance: WorkflowInstance,
    pub events: Vec<WorkflowInstanceEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEventRecord {
    pub created_time: u64,
    pub event: WorkflowInstanceEvent,
}

pub fn handle_workflow_instance_command(
    current: Option<WorkflowInstance>,
    command: WorkflowInstanceCommand,
) -> anyhow::Result<WorkflowTransition> {
    match (current, command) {
        (None, WorkflowInstanceCommand::CreateWorkflow { instance }) => {
            process_create_workflow_command(instance)
        }
        (None, _) => anyhow::bail!("workflow command requires an existing workflow snapshot"),
        (Some(_), WorkflowInstanceCommand::CreateWorkflow { .. }) => {
            anyhow::bail!("workflow already exists")
        }
        (Some(instance), WorkflowInstanceCommand::StartWorkflowRun) => {
            process_workflow_status_command(instance, WorkflowStatus::Running)
        }
        (Some(instance), WorkflowInstanceCommand::CompleteWorkflowRun) => {
            process_workflow_status_command(instance, WorkflowStatus::Completed)
        }
        (Some(instance), WorkflowInstanceCommand::FailWorkflowRun) => {
            process_workflow_status_command(instance, WorkflowStatus::Failed)
        }
        (Some(instance), WorkflowInstanceCommand::PauseWorkflow) => {
            process_pause_workflow_command(instance)
        }
        (Some(instance), WorkflowInstanceCommand::ResumeWorkflow) => {
            process_resume_workflow_command(instance)
        }
        (
            Some(instance),
            WorkflowInstanceCommand::MaterializeTaskAttempts {
                tasks,
                verifier_states,
            },
        ) => process_materialize_task_attempts_command(instance, tasks, verifier_states),
        (Some(instance), WorkflowInstanceCommand::StartTaskAttempt { task_attempt_id }) => {
            process_start_task_attempt_command(instance, task_attempt_id)
        }
        (
            Some(instance),
            WorkflowInstanceCommand::RecordTaskInputValidationFailed {
                task_attempt_id,
                input_data,
                input_mapping,
            },
        ) => process_record_task_failed_command(
            instance,
            task_attempt_id,
            input_data,
            input_mapping,
            true,
        ),
        (
            Some(instance),
            WorkflowInstanceCommand::RecordTaskInputNeeded {
                task_attempt_id,
                input_data,
                input_mapping,
                input_request,
            },
        ) => process_record_task_input_needed_command(
            instance,
            task_attempt_id,
            input_data,
            input_mapping,
            input_request,
        ),
        (
            Some(instance),
            WorkflowInstanceCommand::RecordTaskFailed {
                task_attempt_id,
                input_data,
                input_mapping,
                mark_unsatisfied,
            },
        ) => process_record_task_failed_command(
            instance,
            task_attempt_id,
            input_data,
            input_mapping,
            mark_unsatisfied,
        ),
        (
            Some(instance),
            WorkflowInstanceCommand::RecordTaskSucceeded {
                task_attempt_id,
                input_data,
                input_mapping,
                output_data,
                satisfaction_status,
                verifier_outcome,
            },
        ) => process_record_task_succeeded_command(
            instance,
            task_attempt_id,
            input_data,
            input_mapping,
            output_data,
            satisfaction_status,
            verifier_outcome,
        ),
        (
            Some(instance),
            WorkflowInstanceCommand::SubmitHumanInput {
                task_attempt_id,
                submitted_input,
            },
        ) => process_submit_human_input_command(instance, task_attempt_id, submitted_input),
        (Some(instance), WorkflowInstanceCommand::RetryTask { task_attempt_id }) => {
            process_retry_task_command(instance, task_attempt_id)
        }
        (
            Some(instance),
            WorkflowInstanceCommand::ForceRetryTask {
                task_attempt_id,
                target_host_id,
            },
        ) => process_force_retry_task_command(instance, task_attempt_id, target_host_id),
        (Some(instance), WorkflowInstanceCommand::ApplyStartupRecovery) => {
            process_startup_recovery_command(instance)
        }
    }
}

pub fn apply_workflow_instance_event(
    instance: &mut WorkflowInstance,
    event: &WorkflowInstanceEvent,
) -> anyhow::Result<()> {
    match event {
        WorkflowInstanceEvent::WorkflowCreated { .. } => {
            anyhow::bail!("workflow_created can only initialize an empty workflow snapshot")
        }
        WorkflowInstanceEvent::WorkflowRunStarted => {
            instance.status = WorkflowStatus::Running;
        }
        WorkflowInstanceEvent::WorkflowRunCompleted => {
            instance.status = WorkflowStatus::Completed;
        }
        WorkflowInstanceEvent::WorkflowRunFailed => {
            instance.status = WorkflowStatus::Failed;
        }
        WorkflowInstanceEvent::WorkflowPaused => {
            instance.status = WorkflowStatus::Paused;
        }
        WorkflowInstanceEvent::WorkflowResumed => {
            instance.status = WorkflowStatus::Pending;
        }
        WorkflowInstanceEvent::TaskAttemptsMaterialized {
            tasks,
            verifier_states,
        } => {
            for state in verifier_states {
                instance
                    .verifier_states
                    .insert(state.verifier_task_id.clone(), state.clone());
            }
            for task in tasks {
                let task_attempt_id =
                    TaskInstance::make_task_attempt_id(&task.task_def_id, task.generation_index);
                instance.tasks.insert(task_attempt_id, task.clone());
            }
        }
        WorkflowInstanceEvent::TaskAttemptStarted { task_attempt_id } => {
            task_mut(instance, task_attempt_id)?.status = TaskStatus::Running;
        }
        WorkflowInstanceEvent::TaskInputValidationFailed {
            task_attempt_id,
            input_data,
            input_mapping,
        } => {
            apply_task_inputs(instance, task_attempt_id, input_data, input_mapping)?;
            let task = task_mut(instance, task_attempt_id)?;
            task.status = TaskStatus::Failed;
            task.satisfaction_status = TaskSatisfactionStatus::Unsatisfied;
            instance.status = WorkflowStatus::Failed;
        }
        WorkflowInstanceEvent::TaskInputNeeded {
            task_attempt_id,
            input_data,
            input_mapping,
            input_request,
        } => {
            apply_task_inputs(instance, task_attempt_id, input_data, input_mapping)?;
            task_mut(instance, task_attempt_id)?.status = TaskStatus::InputNeeded {
                input_request: input_request.clone(),
            };
            instance.status = WorkflowStatus::InputNeeded;
        }
        WorkflowInstanceEvent::TaskAttemptFailed {
            task_attempt_id,
            input_data,
            input_mapping,
            mark_unsatisfied,
        } => {
            apply_task_inputs(instance, task_attempt_id, input_data, input_mapping)?;
            let task = task_mut(instance, task_attempt_id)?;
            task.status = TaskStatus::Failed;
            if *mark_unsatisfied {
                task.satisfaction_status = TaskSatisfactionStatus::Unsatisfied;
            }
            instance.status = WorkflowStatus::Failed;
        }
        WorkflowInstanceEvent::TaskAttemptSucceeded {
            task_attempt_id,
            input_data,
            input_mapping,
            output_data,
            satisfaction_status,
            verifier_outcome,
        } => {
            apply_task_inputs(instance, task_attempt_id, input_data, input_mapping)?;
            let task = task_mut(instance, task_attempt_id)?;
            task.status = TaskStatus::Completed;
            if let Some(satisfaction_status) = satisfaction_status {
                task.satisfaction_status = satisfaction_status.clone();
            }
            if let Some(output_data) = output_data {
                task.output_data = Some(output_data.clone());
            }
            if let Some(verifier_outcome) = verifier_outcome {
                apply_task_verifier_outcome(instance, task_attempt_id, verifier_outcome)?;
            }
        }
        WorkflowInstanceEvent::HumanInputSubmitted {
            task_attempt_id,
            submitted_input,
        } => {
            let current_task = task_ref(instance, task_attempt_id)?;
            let continuation_generation = current_task.generation_index + 1;
            let continuation_task = TaskInstance {
                task_def_id: current_task.task_def_id.clone(),
                status: TaskStatus::Pending,
                satisfaction_status: TaskSatisfactionStatus::Pending,
                human_input: Some(submitted_input.clone()),
                input_data: vec![],
                input_mapping: vec![],
                output_data: None,
                generation_index: continuation_generation,
                verifier_metadata: None,
            };
            let continuation_task_attempt_id = TaskInstance::make_task_attempt_id(
                &continuation_task.task_def_id,
                continuation_task.generation_index,
            );
            instance
                .tasks
                .insert(continuation_task_attempt_id, continuation_task);
            instance.status = WorkflowStatus::Pending;
        }
        WorkflowInstanceEvent::TaskRetryStarted { task_attempt_id } => {
            reset_task_for_retry(instance, task_attempt_id)?;
        }
        WorkflowInstanceEvent::TaskForceRetryStarted {
            task_attempt_id,
            target_host_id,
            ..
        } => {
            reset_task_for_retry(instance, task_attempt_id)?;
            instance.pinned_worker_host = Some(target_host_id.clone());
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
    }

    Ok(())
}

fn apply_task_verifier_outcome(
    instance: &mut WorkflowInstance,
    task_attempt_id: &str,
    outcome: &TaskVerifierOutcome,
) -> anyhow::Result<()> {
    let verifier_task = task_ref(instance, task_attempt_id)?;
    let verifier_task_id = verifier_task.task_def_id.clone();
    let generation = verifier_task.generation_index;

    match outcome {
        TaskVerifierOutcome::Accepted {
            verifier_metadata,
            satisfied_task_attempt_ids,
        } => {
            set_verifier_state_status(
                instance,
                &verifier_task_id,
                VerifierStateStatus::Accepted,
                Some(generation),
                Some("complete".to_string()),
            )?;
            task_mut(instance, task_attempt_id)?.verifier_metadata =
                Some(verifier_metadata.clone());
            set_task_satisfaction(
                instance,
                satisfied_task_attempt_ids,
                TaskSatisfactionStatus::Satisfied,
            )?;
        }
        TaskVerifierOutcome::RejectedWithRerun {
            verifier_metadata,
            unsatisfied_task_attempt_ids,
            materialized_tasks,
        } => {
            let feedback_entry = VerifierFeedbackEntry {
                generation_index: generation,
                feedback: verifier_metadata.feedback.clone().ok_or_else(|| {
                    anyhow::anyhow!("rejected verifier outcome must include feedback")
                })?,
                verifier_output: verifier_metadata.verifier_output.clone().ok_or_else(|| {
                    anyhow::anyhow!("rejected verifier outcome must include verifier output")
                })?,
            };
            let state = verifier_state_mut(instance, &verifier_task_id)?;
            state.feedback_history.push(feedback_entry);
            state.latest_generation = generation + 1;
            state.status = VerifierStateStatus::Running;
            state.selected_generation = None;
            state.exit_reason = None;
            task_mut(instance, task_attempt_id)?.verifier_metadata =
                Some(verifier_metadata.clone());
            set_task_satisfaction(
                instance,
                unsatisfied_task_attempt_ids,
                TaskSatisfactionStatus::Unsatisfied,
            )?;
            for task in materialized_tasks {
                let task_attempt_id =
                    TaskInstance::make_task_attempt_id(&task.task_def_id, task.generation_index);
                instance.tasks.insert(task_attempt_id, task.clone());
            }
        }
        TaskVerifierOutcome::Invalid { verifier_metadata } => {
            instance.status = WorkflowStatus::Failed;
            let task = task_mut(instance, task_attempt_id)?;
            task.status = TaskStatus::Failed;
            task.satisfaction_status = TaskSatisfactionStatus::Unsatisfied;
            task.verifier_metadata = Some(verifier_metadata.clone());
        }
        TaskVerifierOutcome::Failed {
            status,
            selected_generation,
            exit_reason,
            verifier_metadata,
            unsatisfied_task_attempt_ids,
        } => {
            set_verifier_state_status(
                instance,
                &verifier_task_id,
                status.clone(),
                *selected_generation,
                Some(exit_reason.clone()),
            )?;
            instance.status = WorkflowStatus::Failed;
            let task = task_mut(instance, task_attempt_id)?;
            task.status = TaskStatus::Failed;
            task.satisfaction_status = TaskSatisfactionStatus::Unsatisfied;
            task.verifier_metadata = Some(verifier_metadata.clone());
            set_task_satisfaction(
                instance,
                unsatisfied_task_attempt_ids,
                TaskSatisfactionStatus::Unsatisfied,
            )?;
        }
        TaskVerifierOutcome::ExhaustedAccepted {
            verifier_metadata,
            satisfied_task_attempt_ids,
        } => {
            set_verifier_state_status(
                instance,
                &verifier_task_id,
                VerifierStateStatus::ExhaustedAccepted,
                Some(generation),
                Some("max_iterations_exhausted".to_string()),
            )?;
            task_mut(instance, task_attempt_id)?.verifier_metadata =
                Some(verifier_metadata.clone());
            set_task_satisfaction(
                instance,
                satisfied_task_attempt_ids,
                TaskSatisfactionStatus::Satisfied,
            )?;
        }
    }

    Ok(())
}

fn set_verifier_state_status(
    instance: &mut WorkflowInstance,
    verifier_task_id: &str,
    status: VerifierStateStatus,
    selected_generation: Option<u32>,
    exit_reason: Option<String>,
) -> anyhow::Result<()> {
    let state = verifier_state_mut(instance, verifier_task_id)?;
    state.status = status;
    state.selected_generation = selected_generation;
    state.exit_reason = exit_reason;
    Ok(())
}

fn set_task_satisfaction(
    instance: &mut WorkflowInstance,
    task_attempt_ids: &[String],
    satisfaction_status: TaskSatisfactionStatus,
) -> anyhow::Result<()> {
    for task_attempt_id in task_attempt_ids {
        task_mut(instance, task_attempt_id)?.satisfaction_status = satisfaction_status.clone();
    }
    Ok(())
}

fn apply_task_inputs(
    instance: &mut WorkflowInstance,
    task_attempt_id: &str,
    input_data: &[serde_json::Value],
    input_mapping: &[TaskInputMapping],
) -> anyhow::Result<()> {
    let task = task_mut(instance, task_attempt_id)?;
    task.input_data = input_data.to_vec();
    task.input_mapping = input_mapping.to_vec();
    Ok(())
}

fn reset_task_for_retry(
    instance: &mut WorkflowInstance,
    task_attempt_id: &str,
) -> anyhow::Result<()> {
    let task = task_mut(instance, task_attempt_id)?;
    task.status = TaskStatus::Pending;
    task.satisfaction_status = TaskSatisfactionStatus::Pending;
    task.output_data = None;
    task.verifier_metadata = None;
    instance.status = WorkflowStatus::Pending;
    Ok(())
}

fn process_create_workflow_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    Ok(WorkflowTransition {
        instance: instance.clone(),
        events: vec![WorkflowInstanceEvent::WorkflowCreated { instance }],
    })
}

fn workflow_transition(
    mut instance: WorkflowInstance,
    event: WorkflowInstanceEvent,
) -> anyhow::Result<WorkflowTransition> {
    apply_workflow_instance_event(&mut instance, &event)?;
    Ok(WorkflowTransition {
        instance,
        events: vec![event],
    })
}

fn process_workflow_status_command(
    instance: WorkflowInstance,
    status: WorkflowStatus,
) -> anyhow::Result<WorkflowTransition> {
    let event = match status {
        WorkflowStatus::Running => WorkflowInstanceEvent::WorkflowRunStarted,
        WorkflowStatus::Completed => WorkflowInstanceEvent::WorkflowRunCompleted,
        WorkflowStatus::Failed => WorkflowInstanceEvent::WorkflowRunFailed,
        _ => anyhow::bail!("unsupported workflow status command target"),
    };
    workflow_transition(instance, event)
}

fn process_pause_workflow_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    match instance.status {
        WorkflowStatus::Pending | WorkflowStatus::Running => {
            workflow_transition(instance, WorkflowInstanceEvent::WorkflowPaused)
        }
        WorkflowStatus::Paused => Ok(WorkflowTransition {
            instance,
            events: vec![],
        }),
        _ => anyhow::bail!(
            "workflow instance {} cannot be paused from its current status",
            instance.id
        ),
    }
}

fn process_resume_workflow_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    if instance.status != WorkflowStatus::Paused {
        anyhow::bail!("workflow instance {} is not paused", instance.id);
    }

    workflow_transition(instance, WorkflowInstanceEvent::WorkflowResumed)
}

fn process_materialize_task_attempts_command(
    instance: WorkflowInstance,
    tasks: Vec<TaskInstance>,
    verifier_states: Vec<VerifierGenerationState>,
) -> anyhow::Result<WorkflowTransition> {
    if tasks.is_empty() && verifier_states.is_empty() {
        anyhow::bail!("materialize task attempts command must include work");
    }

    let mut planned_task_attempt_ids = std::collections::HashSet::new();
    for task in &tasks {
        let task_attempt_id =
            TaskInstance::make_task_attempt_id(&task.task_def_id, task.generation_index);
        if instance.tasks.contains_key(&task_attempt_id)
            || !planned_task_attempt_ids.insert(task_attempt_id.clone())
        {
            anyhow::bail!("task attempt {task_attempt_id} already exists");
        }
    }

    workflow_transition(
        instance,
        WorkflowInstanceEvent::TaskAttemptsMaterialized {
            tasks,
            verifier_states,
        },
    )
}

fn process_start_task_attempt_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
) -> anyhow::Result<WorkflowTransition> {
    let task = task_ref(&instance, &task_attempt_id)?;
    if task.status != TaskStatus::Pending {
        anyhow::bail!("task attempt {task_attempt_id} is not pending");
    }
    workflow_transition(
        instance,
        WorkflowInstanceEvent::TaskAttemptStarted { task_attempt_id },
    )
}

fn process_record_task_input_needed_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
    input_request: String,
) -> anyhow::Result<WorkflowTransition> {
    task_ref(&instance, &task_attempt_id)?;
    workflow_transition(
        instance,
        WorkflowInstanceEvent::TaskInputNeeded {
            task_attempt_id,
            input_data,
            input_mapping,
            input_request,
        },
    )
}

fn process_record_task_failed_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
    mark_unsatisfied: bool,
) -> anyhow::Result<WorkflowTransition> {
    task_ref(&instance, &task_attempt_id)?;
    let event = if mark_unsatisfied {
        WorkflowInstanceEvent::TaskInputValidationFailed {
            task_attempt_id,
            input_data,
            input_mapping,
        }
    } else {
        WorkflowInstanceEvent::TaskAttemptFailed {
            task_attempt_id,
            input_data,
            input_mapping,
            mark_unsatisfied,
        }
    };
    workflow_transition(instance, event)
}

fn process_record_task_succeeded_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
    output_data: Option<serde_json::Value>,
    satisfaction_status: Option<TaskSatisfactionStatus>,
    verifier_outcome: Option<TaskVerifierOutcome>,
) -> anyhow::Result<WorkflowTransition> {
    task_ref(&instance, &task_attempt_id)?;
    workflow_transition(
        instance,
        WorkflowInstanceEvent::TaskAttemptSucceeded {
            task_attempt_id,
            input_data,
            input_mapping,
            output_data,
            satisfaction_status,
            verifier_outcome,
        },
    )
}

fn process_submit_human_input_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    submitted_input: serde_json::Value,
) -> anyhow::Result<WorkflowTransition> {
    if instance.status != WorkflowStatus::InputNeeded {
        anyhow::bail!("workflow instance {} is not waiting for input", instance.id);
    }

    let task = task_ref(&instance, &task_attempt_id)?;
    if !matches!(task.status, TaskStatus::InputNeeded { .. }) {
        anyhow::bail!("task attempt {task_attempt_id} is not waiting for input");
    }

    let continuation_generation = task.generation_index + 1;
    let continuation_task_attempt_id =
        TaskInstance::make_task_attempt_id(&task.task_def_id, continuation_generation);
    if instance.tasks.contains_key(&continuation_task_attempt_id) {
        anyhow::bail!("task attempt {continuation_task_attempt_id} already exists");
    }

    workflow_transition(
        instance,
        WorkflowInstanceEvent::HumanInputSubmitted {
            task_attempt_id,
            submitted_input: submitted_input.clone(),
        },
    )
}

// TODO: investigate if re-try should also create new attempt/generation
fn process_retry_task_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
) -> anyhow::Result<WorkflowTransition> {
    validate_retry_task(&instance, &task_attempt_id)?;
    workflow_transition(
        instance,
        WorkflowInstanceEvent::TaskRetryStarted { task_attempt_id },
    )
}

fn process_force_retry_task_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    target_host_id: WorkerHostId,
) -> anyhow::Result<WorkflowTransition> {
    validate_retry_task(&instance, &task_attempt_id)?;
    workflow_transition(
        instance,
        WorkflowInstanceEvent::TaskForceRetryStarted {
            task_attempt_id,
            target_host_id,
        },
    )
}

fn process_startup_recovery_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    workflow_transition(instance, WorkflowInstanceEvent::StartupRecoveryApplied)
}

fn validate_retry_task(instance: &WorkflowInstance, task_attempt_id: &str) -> anyhow::Result<()> {
    if instance.status != WorkflowStatus::Failed {
        anyhow::bail!("workflow instance is not failed");
    }

    let task = task_ref(instance, task_attempt_id)?;
    if task.status != TaskStatus::Failed {
        anyhow::bail!("task attempt {task_attempt_id} is not failed");
    }

    Ok(())
}

fn task_ref<'a>(
    instance: &'a WorkflowInstance,
    task_attempt_id: &str,
) -> anyhow::Result<&'a TaskInstance> {
    instance
        .tasks
        .get(task_attempt_id)
        .ok_or_else(|| anyhow::anyhow!("task attempt {task_attempt_id} not found"))
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
            version: 0,
            status: WorkflowStatus::Pending,
            trigger_input: None,
            pinned_worker_host: None,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    fn task() -> TaskInstance {
        TaskInstance {
            task_def_id: "task-a".to_string(),
            status: TaskStatus::Pending,
            satisfaction_status: TaskSatisfactionStatus::Pending,
            human_input: None,
            input_data: vec![],
            input_mapping: vec![],
            output_data: None,
            generation_index: 1,
            verifier_metadata: None,
        }
    }

    #[test]
    fn reducer_rejects_missing_task_operation_targets() {
        let mut instance = instance();
        let result = apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::TaskAttemptStarted {
                task_attempt_id: "task-a[1]".to_string(),
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn workflow_transition_applies_event_to_returned_instance() {
        let transition =
            workflow_transition(instance(), WorkflowInstanceEvent::WorkflowRunStarted).unwrap();
        assert_eq!(transition.instance.status, WorkflowStatus::Running);
        assert_eq!(transition.events.len(), 1);
    }

    #[test]
    fn reducer_applies_task_success_verifier_outcome() {
        let mut instance = instance();
        instance.tasks.insert(
            "verify[1]".to_string(),
            TaskInstance {
                task_def_id: "verify".to_string(),
                ..task()
            },
        );
        instance.verifier_states.insert(
            "verify".to_string(),
            VerifierGenerationState {
                verifier_task_id: "verify".to_string(),
                rerun_start_task_id: "task-a".to_string(),
                latest_generation: 1,
                selected_generation: None,
                feedback_history: vec![],
                status: VerifierStateStatus::Running,
                exit_reason: None,
            },
        );

        apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::TaskAttemptSucceeded {
                task_attempt_id: "verify[1]".to_string(),
                input_data: vec![serde_json::json!({"input": true})],
                input_mapping: vec![],
                output_data: Some(serde_json::json!({"ok": true})),
                satisfaction_status: Some(TaskSatisfactionStatus::Satisfied),
                verifier_outcome: Some(TaskVerifierOutcome::Accepted {
                    verifier_metadata: VerifierAttemptMetadata {
                        status: crate::core::models::VerifierAttemptStatus::Accepted,
                        decision: None,
                        feedback: None,
                        verifier_output: Some(serde_json::json!({"decision": "complete"})),
                        exit_reason: Some("complete".to_string()),
                    },
                    satisfied_task_attempt_ids: vec!["verify[1]".to_string()],
                }),
            },
        )
        .unwrap();

        let task = &instance.tasks["verify[1]"];
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.satisfaction_status, TaskSatisfactionStatus::Satisfied);
        assert_eq!(task.output_data, Some(serde_json::json!({"ok": true})));
        let state = &instance.verifier_states["verify"];
        assert_eq!(state.status, VerifierStateStatus::Accepted);
        assert_eq!(state.feedback_history.len(), 0);
        assert_eq!(state.selected_generation, Some(1));
    }

    #[test]
    fn reducer_applies_task_materialization_event() {
        let mut instance = instance();

        apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::TaskAttemptsMaterialized {
                tasks: vec![task()],
                verifier_states: vec![VerifierGenerationState {
                    verifier_task_id: "verify".to_string(),
                    rerun_start_task_id: "task-a".to_string(),
                    latest_generation: 1,
                    selected_generation: None,
                    feedback_history: vec![],
                    status: VerifierStateStatus::Running,
                    exit_reason: None,
                }],
            },
        )
        .unwrap();

        assert!(instance.tasks.contains_key("task-a[1]"));
        let state = &instance.verifier_states["verify"];
        assert_eq!(state.status, VerifierStateStatus::Running);
    }

    #[test]
    fn command_applies_startup_recovery() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Running;
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Running,
                ..task()
            },
        );

        let transition = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::ApplyStartupRecovery,
        )
        .unwrap();

        assert_eq!(transition.events.len(), 1);
        assert!(matches!(
            &transition.events[0],
            WorkflowInstanceEvent::StartupRecoveryApplied
        ));
        assert_eq!(transition.instance.status, WorkflowStatus::Pending);
        assert_eq!(
            transition.instance.tasks["task-a[1]"].status,
            TaskStatus::Pending
        );
    }

    #[test]
    fn command_materializes_human_input_continuation() {
        let mut instance = instance();
        instance.status = WorkflowStatus::InputNeeded;
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::InputNeeded {
                    input_request: "need input".to_string(),
                },
                ..task()
            },
        );

        let transition = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::SubmitHumanInput {
                task_attempt_id: "task-a[1]".to_string(),
                submitted_input: serde_json::json!({"answer": "ship it"}),
            },
        )
        .unwrap();

        assert_eq!(transition.events.len(), 1);
        assert!(matches!(
            &transition.events[0],
            WorkflowInstanceEvent::HumanInputSubmitted {
                task_attempt_id,
                ..
            } if task_attempt_id == "task-a[1]"
        ));
        let instance = transition.instance;
        assert_eq!(instance.status, WorkflowStatus::Pending);
        assert!(matches!(
            instance.tasks["task-a[1]"].status,
            TaskStatus::InputNeeded { .. }
        ));
        assert_eq!(instance.tasks["task-a[2]"].task_def_id, "task-a");
        assert_eq!(instance.tasks["task-a[2]"].status, TaskStatus::Pending);
        assert_eq!(instance.tasks["task-a[2]"].generation_index, 2);
        assert_eq!(
            instance.tasks["task-a[2]"].human_input,
            Some(serde_json::json!({"answer": "ship it"}))
        );
    }

    #[test]
    fn command_applies_task_retry() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Failed;
        instance.pinned_worker_host = Some(WorkerHostId::new("host-a"));
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Failed,
                satisfaction_status: TaskSatisfactionStatus::Unsatisfied,
                input_data: vec![serde_json::json!({"input": true})],
                output_data: Some(serde_json::json!({"stale": true})),
                verifier_metadata: Some(VerifierAttemptMetadata {
                    status: crate::core::models::VerifierAttemptStatus::Invalid,
                    decision: None,
                    feedback: None,
                    verifier_output: Some(serde_json::json!({"bad": true})),
                    exit_reason: Some("invalid".to_string()),
                }),
                ..task()
            },
        );

        let transition = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::RetryTask {
                task_attempt_id: "task-a[1]".to_string(),
            },
        )
        .unwrap();

        assert_eq!(transition.events.len(), 1);
        assert!(matches!(
            &transition.events[0],
            WorkflowInstanceEvent::TaskRetryStarted { task_attempt_id }
                if task_attempt_id == "task-a[1]"
        ));
        let instance = transition.instance;
        let task = &instance.tasks["task-a[1]"];
        assert_eq!(instance.status, WorkflowStatus::Pending);
        assert_eq!(
            instance.pinned_worker_host,
            Some(WorkerHostId::new("host-a"))
        );
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.satisfaction_status, TaskSatisfactionStatus::Pending);
        assert_eq!(task.output_data, None);
        assert_eq!(task.verifier_metadata, None);
    }

    #[test]
    fn command_rejects_retry_for_non_failed_task() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Failed;
        instance
            .tasks
            .insert("task-a[1]".to_string(), TaskInstance { ..task() });

        let error = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::RetryTask {
                task_attempt_id: "task-a[1]".to_string(),
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("is not failed"));
    }

    #[test]
    fn command_applies_force_retry_with_same_host() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Failed;
        instance.pinned_worker_host = Some(WorkerHostId::new("host-a"));
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Failed,
                satisfaction_status: TaskSatisfactionStatus::Unsatisfied,
                output_data: Some(serde_json::json!({"stale": true})),
                ..task()
            },
        );

        let transition = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::ForceRetryTask {
                task_attempt_id: "task-a[1]".to_string(),
                target_host_id: WorkerHostId::new("host-a"),
            },
        )
        .unwrap();

        assert_eq!(transition.events.len(), 1);
        let instance = transition.instance;
        assert_eq!(instance.status, WorkflowStatus::Pending);
        assert_eq!(
            instance.pinned_worker_host,
            Some(WorkerHostId::new("host-a"))
        );
        assert_eq!(instance.tasks["task-a[1]"].status, TaskStatus::Pending);
        assert_eq!(instance.tasks["task-a[1]"].output_data, None);
    }

    #[test]
    fn command_applies_force_retry_with_reassigned_host() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Failed;
        instance.pinned_worker_host = Some(WorkerHostId::new("host-a"));
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Failed,
                satisfaction_status: TaskSatisfactionStatus::Unsatisfied,
                ..task()
            },
        );

        let transition = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::ForceRetryTask {
                task_attempt_id: "task-a[1]".to_string(),
                target_host_id: WorkerHostId::new("host-b"),
            },
        )
        .unwrap();

        let instance = transition.instance;
        assert_eq!(instance.status, WorkflowStatus::Pending);
        assert_eq!(
            instance.pinned_worker_host,
            Some(WorkerHostId::new("host-b"))
        );
        assert_eq!(instance.tasks["task-a[1]"].status, TaskStatus::Pending);
    }

    #[test]
    fn command_derives_force_retry_pin_change_event_from_instance() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Failed;
        instance.pinned_worker_host = Some(WorkerHostId::new("host-a"));
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Failed,
                ..task()
            },
        );

        let transition = handle_workflow_instance_command(
            Some(instance),
            WorkflowInstanceCommand::ForceRetryTask {
                task_attempt_id: "task-a[1]".to_string(),
                target_host_id: WorkerHostId::new("host-b"),
            },
        )
        .unwrap();

        assert!(matches!(
            &transition.events[0],
            WorkflowInstanceEvent::TaskForceRetryStarted {
                task_attempt_id,
                target_host_id,
            } if task_attempt_id == "task-a[1]"
                && target_host_id == &WorkerHostId::new("host-b")
        ));
    }

    #[test]
    fn reducer_applies_force_retry_pin_change() {
        let mut instance = instance();
        instance.status = WorkflowStatus::Failed;
        instance.pinned_worker_host = Some(WorkerHostId::new("host-a"));
        instance.tasks.insert(
            "task-a[1]".to_string(),
            TaskInstance {
                status: TaskStatus::Failed,
                ..task()
            },
        );

        apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::TaskForceRetryStarted {
                task_attempt_id: "task-a[1]".to_string(),
                target_host_id: WorkerHostId::new("host-b"),
            },
        )
        .unwrap();

        assert_eq!(
            instance.pinned_worker_host,
            Some(WorkerHostId::new("host-b"))
        );
    }
}
