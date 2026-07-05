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
    /// Changes the overall workflow instance status.
    WorkflowStatusChanged { status: WorkflowStatus },
    /// Changes the workflow instance pinned worker host.
    WorkflowPinnedHostChanged {
        previous_host_id: Option<WorkerHostId>,
        target_host_id: WorkerHostId,
        local_context_may_be_lost: bool,
    },
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
        verifier_events: Vec<WorkflowInstanceEvent>,
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

struct WorkflowTransitionBuilder {
    instance: WorkflowInstance,
    events: Vec<WorkflowInstanceEvent>,
}

impl WorkflowTransitionBuilder {
    fn from_instance(instance: WorkflowInstance) -> Self {
        Self {
            instance,
            events: vec![],
        }
    }

    fn instance(&self) -> &WorkflowInstance {
        &self.instance
    }

    fn record_event(&mut self, event: WorkflowInstanceEvent) -> anyhow::Result<()> {
        apply_workflow_instance_event(&mut self.instance, &event)?;
        self.events.push(event);
        Ok(())
    }

    fn finish(self) -> WorkflowTransition {
        WorkflowTransition {
            instance: self.instance,
            events: self.events,
        }
    }
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
                verifier_events,
            },
        ) => process_record_task_succeeded_command(
            instance,
            task_attempt_id,
            input_data,
            input_mapping,
            output_data,
            satisfaction_status,
            verifier_events,
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
        WorkflowInstanceEvent::WorkflowStatusChanged { status } => {
            instance.status = status.clone();
        }
        WorkflowInstanceEvent::WorkflowPinnedHostChanged { target_host_id, .. } => {
            instance.pinned_worker_host = Some(target_host_id.clone());
        }
        WorkflowInstanceEvent::TaskMaterialized {
            task_attempt_id,
            task,
        } => {
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

fn process_create_workflow_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    Ok(WorkflowTransition {
        instance: instance.clone(),
        events: vec![WorkflowInstanceEvent::WorkflowCreated { instance }],
    })
}

fn process_workflow_status_command(
    instance: WorkflowInstance,
    status: WorkflowStatus,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged { status })?;
    Ok(transition.finish())
}

fn process_pause_workflow_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);

    match transition.instance().status {
        WorkflowStatus::Pending | WorkflowStatus::Running => {
            transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
                status: WorkflowStatus::Paused,
            })?;
        }
        WorkflowStatus::Paused => {}
        _ => anyhow::bail!(
            "workflow instance {} cannot be paused from its current status",
            transition.instance().id
        ),
    }

    Ok(transition.finish())
}

fn process_resume_workflow_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);

    if transition.instance().status != WorkflowStatus::Paused {
        anyhow::bail!(
            "workflow instance {} is not paused",
            transition.instance().id
        );
    }

    transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
        status: WorkflowStatus::Pending,
    })?;

    Ok(transition.finish())
}

fn process_materialize_task_attempts_command(
    instance: WorkflowInstance,
    tasks: Vec<TaskInstance>,
    verifier_states: Vec<VerifierGenerationState>,
) -> anyhow::Result<WorkflowTransition> {
    if tasks.is_empty() && verifier_states.is_empty() {
        anyhow::bail!("materialize task attempts command must include work");
    }

    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    let mut planned_task_attempt_ids = std::collections::HashSet::new();
    for state in verifier_states {
        let verifier_task_id = state.verifier_task_id.clone();
        transition.record_event(WorkflowInstanceEvent::VerifierStateUpserted {
            verifier_task_id,
            state,
        })?;
    }
    for task in tasks {
        let task_attempt_id =
            TaskInstance::make_task_attempt_id(&task.task_def_id, task.generation_index);
        if transition.instance().tasks.contains_key(&task_attempt_id)
            || !planned_task_attempt_ids.insert(task_attempt_id.clone())
        {
            anyhow::bail!("task attempt {task_attempt_id} already exists");
        }
        transition.record_event(WorkflowInstanceEvent::TaskMaterialized {
            task_attempt_id,
            task,
        })?;
    }

    Ok(transition.finish())
}

fn process_start_task_attempt_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    let task = task_ref(transition.instance(), &task_attempt_id)?;
    if task.status != TaskStatus::Pending {
        anyhow::bail!("task attempt {task_attempt_id} is not pending");
    }
    transition.record_event(WorkflowInstanceEvent::TaskStatusChanged {
        task_attempt_id,
        status: TaskStatus::Running,
    })?;
    Ok(transition.finish())
}

fn process_record_task_input_needed_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
    input_request: String,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    task_ref(transition.instance(), &task_attempt_id)?;
    record_task_inputs(&mut transition, &task_attempt_id, input_data, input_mapping)?;
    transition.record_event(WorkflowInstanceEvent::TaskStatusChanged {
        task_attempt_id,
        status: TaskStatus::InputNeeded { input_request },
    })?;
    transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
        status: WorkflowStatus::InputNeeded,
    })?;
    Ok(transition.finish())
}

fn process_record_task_failed_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
    mark_unsatisfied: bool,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    task_ref(transition.instance(), &task_attempt_id)?;
    record_task_inputs(&mut transition, &task_attempt_id, input_data, input_mapping)?;
    transition.record_event(WorkflowInstanceEvent::TaskStatusChanged {
        task_attempt_id: task_attempt_id.clone(),
        status: TaskStatus::Failed,
    })?;
    if mark_unsatisfied {
        transition.record_event(WorkflowInstanceEvent::TaskSatisfactionChanged {
            task_attempt_id: task_attempt_id.clone(),
            satisfaction_status: TaskSatisfactionStatus::Unsatisfied,
        })?;
    }
    transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
        status: WorkflowStatus::Failed,
    })?;
    Ok(transition.finish())
}

fn process_record_task_succeeded_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
    output_data: Option<serde_json::Value>,
    satisfaction_status: Option<TaskSatisfactionStatus>,
    verifier_events: Vec<WorkflowInstanceEvent>,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    task_ref(transition.instance(), &task_attempt_id)?;
    record_task_inputs(&mut transition, &task_attempt_id, input_data, input_mapping)?;

    transition.record_event(WorkflowInstanceEvent::TaskStatusChanged {
        task_attempt_id: task_attempt_id.clone(),
        status: TaskStatus::Completed,
    })?;

    if let Some(satisfaction_status) = satisfaction_status {
        transition.record_event(WorkflowInstanceEvent::TaskSatisfactionChanged {
            task_attempt_id: task_attempt_id.clone(),
            satisfaction_status,
        })?;
    }

    if let Some(output_data) = output_data {
        transition.record_event(WorkflowInstanceEvent::TaskOutputRecorded {
            task_attempt_id,
            output_data: Some(output_data),
        })?;
    }

    for event in verifier_events {
        transition.record_event(event)?;
    }
    Ok(transition.finish())
}

fn process_submit_human_input_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    submitted_input: serde_json::Value,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);

    if transition.instance().status != WorkflowStatus::InputNeeded {
        anyhow::bail!(
            "workflow instance {} is not waiting for input",
            transition.instance().id
        );
    }

    let task = task_ref(transition.instance(), &task_attempt_id)?;
    if !matches!(task.status, TaskStatus::InputNeeded { .. }) {
        anyhow::bail!("task attempt {task_attempt_id} is not waiting for input");
    }

    let continuation_generation = task.generation_index + 1;
    let task_def_id = task.task_def_id.clone();
    let continuation_task_attempt_id =
        TaskInstance::make_task_attempt_id(&task.task_def_id, continuation_generation);
    if transition
        .instance()
        .tasks
        .contains_key(&continuation_task_attempt_id)
    {
        anyhow::bail!("task attempt {continuation_task_attempt_id} already exists");
    }

    transition.record_event(WorkflowInstanceEvent::TaskMaterialized {
        task_attempt_id: continuation_task_attempt_id,
        task: TaskInstance {
            task_def_id,
            status: TaskStatus::Pending,
            satisfaction_status: TaskSatisfactionStatus::Pending,
            human_input: Some(submitted_input),
            input_data: vec![],
            input_mapping: vec![],
            output_data: None,
            generation_index: continuation_generation,
            verifier_metadata: None,
        },
    })?;
    transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
        status: WorkflowStatus::Pending,
    })?;

    Ok(transition.finish())
}

// TODO: investigate if re-try should also create new attempt/generation
fn process_retry_task_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    record_retry_task_events(&mut transition, task_attempt_id)?;
    Ok(transition.finish())
}

fn process_force_retry_task_command(
    instance: WorkflowInstance,
    task_attempt_id: String,
    target_host_id: WorkerHostId,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);
    let previous_host_id = transition.instance().pinned_worker_host.clone();
    let expected_context_may_be_lost = previous_host_id.as_ref() != Some(&target_host_id);

    record_retry_task_events(&mut transition, task_attempt_id)?;
    transition.record_event(WorkflowInstanceEvent::WorkflowPinnedHostChanged {
        previous_host_id,
        target_host_id,
        local_context_may_be_lost: expected_context_may_be_lost,
    })?;

    Ok(transition.finish())
}

fn process_startup_recovery_command(
    instance: WorkflowInstance,
) -> anyhow::Result<WorkflowTransition> {
    let mut transition = WorkflowTransitionBuilder::from_instance(instance);

    if transition.instance().status == WorkflowStatus::Running {
        transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
            status: WorkflowStatus::Pending,
        })?;
    }

    let mut running_task_attempt_ids = transition
        .instance()
        .tasks
        .iter()
        .filter_map(|(task_attempt_id, task)| {
            (task.status == TaskStatus::Running).then_some(task_attempt_id.clone())
        })
        .collect::<Vec<_>>();
    running_task_attempt_ids.sort();

    for task_attempt_id in running_task_attempt_ids {
        transition.record_event(WorkflowInstanceEvent::TaskStatusChanged {
            task_attempt_id,
            status: TaskStatus::Pending,
        })?;
    }

    Ok(transition.finish())
}

fn record_retry_task_events(
    transition: &mut WorkflowTransitionBuilder,
    task_attempt_id: String,
) -> anyhow::Result<()> {
    if transition.instance().status != WorkflowStatus::Failed {
        anyhow::bail!("workflow instance is not failed");
    }

    let task = task_ref(transition.instance(), &task_attempt_id)?;
    if task.status != TaskStatus::Failed {
        anyhow::bail!("task attempt {task_attempt_id} is not failed");
    }

    transition.record_event(WorkflowInstanceEvent::TaskStatusChanged {
        task_attempt_id: task_attempt_id.clone(),
        status: TaskStatus::Pending,
    })?;
    transition.record_event(WorkflowInstanceEvent::TaskSatisfactionChanged {
        task_attempt_id: task_attempt_id.clone(),
        satisfaction_status: TaskSatisfactionStatus::Pending,
    })?;
    transition.record_event(WorkflowInstanceEvent::TaskOutputRecorded {
        task_attempt_id: task_attempt_id.clone(),
        output_data: None,
    })?;
    transition.record_event(WorkflowInstanceEvent::TaskVerifierMetadataSet {
        task_attempt_id,
        verifier_metadata: None,
    })?;
    transition.record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
        status: WorkflowStatus::Pending,
    })?;

    Ok(())
}

fn record_task_inputs(
    transition: &mut WorkflowTransitionBuilder,
    task_attempt_id: &str,
    input_data: Vec<serde_json::Value>,
    input_mapping: Vec<TaskInputMapping>,
) -> anyhow::Result<()> {
    transition.record_event(WorkflowInstanceEvent::TaskInputDataSet {
        task_attempt_id: task_attempt_id.to_string(),
        input_data,
    })?;
    transition.record_event(WorkflowInstanceEvent::TaskInputMappingSet {
        task_attempt_id: task_attempt_id.to_string(),
        input_mapping,
    })?;
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

    fn apply_events(
        mut instance: WorkflowInstance,
        events: &[WorkflowInstanceEvent],
    ) -> anyhow::Result<WorkflowInstance> {
        for event in events {
            apply_workflow_instance_event(&mut instance, event)?;
        }
        Ok(instance)
    }

    #[test]
    fn reducer_applies_ordered_task_events() {
        let instance = apply_events(
            instance(),
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
    fn transition_builder_record_event_updates_working_instance_immediately() {
        let mut transition = WorkflowTransitionBuilder::from_instance(instance());

        transition
            .record_event(WorkflowInstanceEvent::WorkflowStatusChanged {
                status: WorkflowStatus::Running,
            })
            .unwrap();

        assert_eq!(transition.instance().status, WorkflowStatus::Running);
        assert_eq!(transition.events.len(), 1);

        let transition = transition.finish();
        assert_eq!(transition.instance.status, WorkflowStatus::Running);
        assert_eq!(transition.events.len(), 1);
    }

    #[test]
    fn reducer_applies_verifier_events() {
        let instance = apply_events(
            instance(),
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

        assert_eq!(transition.events.len(), 2);
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

        assert_eq!(transition.events.len(), 2);
        assert!(matches!(
            &transition.events[0],
            WorkflowInstanceEvent::TaskMaterialized { task_attempt_id, .. }
                if task_attempt_id == "task-a[2]"
        ));
        assert!(matches!(
            &transition.events[1],
            WorkflowInstanceEvent::WorkflowStatusChanged {
                status: WorkflowStatus::Pending
            }
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

        assert_eq!(transition.events.len(), 5);
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

        assert_eq!(transition.events.len(), 6);
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

        assert!(transition.events.iter().any(|event| matches!(
            event,
            WorkflowInstanceEvent::WorkflowPinnedHostChanged {
                previous_host_id,
                target_host_id,
                local_context_may_be_lost,
            } if previous_host_id == &Some(WorkerHostId::new("host-a"))
                && target_host_id == &WorkerHostId::new("host-b")
                && *local_context_may_be_lost
        )));
    }

    #[test]
    fn reducer_applies_workflow_pin_change() {
        let mut instance = instance();
        instance.pinned_worker_host = Some(WorkerHostId::new("host-a"));

        apply_workflow_instance_event(
            &mut instance,
            &WorkflowInstanceEvent::WorkflowPinnedHostChanged {
                previous_host_id: Some(WorkerHostId::new("host-a")),
                target_host_id: WorkerHostId::new("host-b"),
                local_context_may_be_lost: true,
            },
        )
        .unwrap();

        assert_eq!(
            instance.pinned_worker_host,
            Some(WorkerHostId::new("host-b"))
        );
    }
}
