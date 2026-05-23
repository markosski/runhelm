use crate::core::function_resolution::resolve_task_function_ref;
use crate::core::models::{
    ExecutionMetadata, LoopExecutionContext, LoopFeedbackEntry, TaskGenerationMetadata,
    TaskInstance, TaskStatus, TaskStatusReport, VerifierAttemptMetadata, VerifierAttemptStatus,
    VerifierControlConfig, VerifierDecision, VerifierExecutionResult, VerifierGenerationState,
    VerifierStateStatus, VerifierStatusReport, WorkflowDef, WorkflowInstance, WorkflowStatus,
    WorkflowStatusReport,
};
use crate::ports::executor::{ExecutionResult, ExecutorPort};
use crate::ports::storage::StoragePort;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct WorkflowEngine {
    storage: Arc<dyn StoragePort + Send + Sync>,
    executor: Arc<dyn ExecutorPort + Send + Sync>,
}

impl WorkflowEngine {
    pub fn new(
        storage: Arc<dyn StoragePort + Send + Sync>,
        executor: Arc<dyn ExecutorPort + Send + Sync>,
    ) -> Self {
        Self { storage, executor }
    }

    /// Returns a lightweight status snapshot of a workflow instance.
    /// Reads the latest persisted state from storage — safe to call at any time,
    /// including while `run_workflow_instance` is executing.
    pub async fn get_workflow_status(
        &self,
        instance_id: &str,
    ) -> anyhow::Result<Option<WorkflowStatusReport>> {
        let Some(instance) = self.storage.get_workflow_instance(instance_id).await? else {
            return Ok(None);
        };

        let mut tasks: Vec<TaskStatusReport> = instance
            .tasks
            .iter()
            .map(|(id, t)| TaskStatusReport {
                task_id: id.clone(),
                task_def_id: t.task_def_id.clone(),
                status: t.status.clone(),
                has_output: t.output_data.is_some(),
                generation: Some(t.generation.clone()),
                verifier_metadata: t.verifier_metadata.clone(),
            })
            .collect();

        // Sort for deterministic ordering.
        tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        let mut verifier_states: Vec<VerifierStatusReport> = instance
            .verifier_states
            .values()
            .map(|state| VerifierStatusReport {
                verifier_task_id: state.verifier_task_id.clone(),
                rerun_start_task_id: state.rerun_start_task_id.clone(),
                latest_generation: state.latest_generation,
                selected_generation: state.selected_generation,
                status: state.status.clone(),
                feedback_count: state.feedback_history.len(),
                exit_reason: state.exit_reason.clone(),
            })
            .collect();
        verifier_states.sort_by(|a, b| a.verifier_task_id.cmp(&b.verifier_task_id));

        Ok(Some(WorkflowStatusReport {
            instance_id: instance.id,
            workflow_def_id: instance.workflow_def_id,
            status: instance.status,
            tasks,
            verifier_states,
        }))
    }

    pub async fn run_workflow_instance(&self, instance_id: String) -> anyhow::Result<()> {
        let mut instance = match self.storage.get_workflow_instance(&instance_id).await? {
            Some(i) => i,
            None => anyhow::bail!("Workflow instance not found"),
        };

        let def = match self
            .storage
            .get_workflow_def(&instance.workflow_def_id)
            .await?
        {
            Some(d) => d,
            None => anyhow::bail!("Workflow definition not found"),
        };

        instance.status = WorkflowStatus::Running;
        self.storage
            .save_workflow_instance(instance.clone())
            .await?;

        let loop_slices = self.compute_loop_slices(&def);

        // Initialize tasks if not already done
        if instance.tasks.is_empty() {
            for task_def in &def.tasks {
                let attempt_id = Self::generation_attempt_id(&task_def.id, 1);
                instance.tasks.insert(
                    attempt_id.clone(),
                    TaskInstance {
                        task_def_id: task_def.id.clone(),
                        status: TaskStatus::Pending,
                        input_data: vec![], // Empty until upstream dependencies propagate data
                        output_data: None,
                        recorded_side_effects: vec![],
                        generation: TaskGenerationMetadata {
                            attempt_id,
                            original_task_def_id: task_def.id.clone(),
                            generation_index: 1,
                        },
                        verifier_metadata: None,
                    },
                );
            }
            self.storage
                .save_workflow_instance(instance.clone())
                .await?;
        }

        // Main Execution Loop
        let mut progress_made = true;
        while progress_made {
            progress_made = false;

            if self.materialize_eligible_generations(&mut instance, &def, &loop_slices) {
                progress_made = true;
            }

            let mut tasks_to_run = Vec::new();

            for (task_id, task_instance) in instance.tasks.iter() {
                if task_instance.status == TaskStatus::Pending {
                    let task_def = def
                        .tasks
                        .iter()
                        .find(|t| t.id == task_instance.task_def_id)
                        .unwrap();
                    let can_run = self
                        .resolve_inputs(&instance, &def, task_id, task_instance, task_def)
                        .is_some();
                    if can_run {
                        tasks_to_run.push(task_id.clone());
                    }
                }
            }

            for task_id in tasks_to_run {
                // Transition to Running
                if let Some(task) = instance.tasks.get_mut(&task_id) {
                    task.status = TaskStatus::Running;
                }
                progress_made = true;

                let task_instance = instance.tasks.get(&task_id).cloned().unwrap();
                let task_def = def
                    .tasks
                    .iter()
                    .find(|t| t.id == task_instance.task_def_id)
                    .unwrap();

                let inputs = self
                    .resolve_inputs(&instance, &def, &task_id, &task_instance, task_def)
                    .unwrap_or_default();

                if let Err(error) = validate_inputs(task_def, &inputs) {
                    if let Some(task) = instance.tasks.get_mut(&task_id) {
                        task.status = TaskStatus::Failed;
                        task.input_data = inputs.clone();
                    }
                    instance.status = WorkflowStatus::Failed;
                    self.storage
                        .save_workflow_instance(instance.clone())
                        .await?;
                    return Err(error);
                }

                let metadata =
                    self.execution_metadata(&instance, &def, &task_id, &task_instance, task_def);
                if let Some(task) = instance.tasks.get_mut(&task_id) {
                    task.input_data = inputs.clone();
                }

                let execution_result =
                    match resolve_task_function_ref(self.storage.as_ref(), task_def).await {
                        Ok(resolved_task_def) => {
                            self.executor
                                .execute_with_metadata(&resolved_task_def, &inputs, &metadata)
                                .await
                        }
                        Err(error) => Err(error),
                    };

                match execution_result {
                    Ok(result) => {
                        let output = match result {
                            ExecutionResult::Success(output) => output,
                            ExecutionResult::InputNeeded(description) => {
                                if let Some(task) = instance.tasks.get_mut(&task_id) {
                                    task.status = TaskStatus::InputNeeded { description };
                                }
                                instance.status = WorkflowStatus::InputNeeded;
                                continue;
                            }
                            ExecutionResult::Failure(reason) => {
                                if let Some(task) = instance.tasks.get_mut(&task_id) {
                                    task.status = TaskStatus::Failed;
                                }
                                instance.status = WorkflowStatus::Failed;
                                self.storage
                                    .save_workflow_instance(instance.clone())
                                    .await?;
                                anyhow::bail!("Task execution failed: {}", reason);
                            }
                        };

                        // Validate against output_schema if one is defined; skip for side-effect-only tasks.
                        let output_schema = effective_output_schema(task_def);
                        let schema_ok = match output_schema {
                            Some(schema) => match jsonschema::validator_for(schema) {
                                Ok(validator) => validator.is_valid(&output),
                                Err(_) => false,
                            },
                            None => true,
                        };

                        if schema_ok {
                            if let Some(task) = instance.tasks.get_mut(&task_id) {
                                task.status = TaskStatus::Completed;
                                // Only record output when a schema is declared.
                                if output_schema.is_some() {
                                    task.output_data = Some(output.clone());
                                }
                            }
                            if task_verifier(task_def).is_some() {
                                let verifier_result = match verifier_result_from_output(&output) {
                                    Ok(verifier_result) => verifier_result,
                                    Err(error) => {
                                        if let Some(task) = instance.tasks.get_mut(&task_id) {
                                            task.status = TaskStatus::Failed;
                                            task.verifier_metadata =
                                                Some(VerifierAttemptMetadata {
                                                    status: VerifierAttemptStatus::Invalid,
                                                    decision: None,
                                                    feedback: None,
                                                    verifier_output: Some(output.clone()),
                                                    exit_reason: Some(error.to_string()),
                                                });
                                        }
                                        instance.status = WorkflowStatus::Failed;
                                        self.storage
                                            .save_workflow_instance(instance.clone())
                                            .await?;
                                        return Err(error);
                                    }
                                };
                                if let Err(error) = self.apply_verifier_result(
                                    &mut instance,
                                    &def,
                                    &loop_slices,
                                    &task_id,
                                    &output,
                                    verifier_result,
                                ) {
                                    self.storage
                                        .save_workflow_instance(instance.clone())
                                        .await?;
                                    return Err(error);
                                }
                            }
                        } else {
                            if let Some(task) = instance.tasks.get_mut(&task_id) {
                                task.status = TaskStatus::Failed;
                            }
                            instance.status = WorkflowStatus::Failed;
                            self.storage
                                .save_workflow_instance(instance.clone())
                                .await?;
                            anyhow::bail!("Task output failed schema validation");
                        }
                    }
                    Err(e) => {
                        if let Some(task) = instance.tasks.get_mut(&task_id) {
                            task.status = TaskStatus::Failed;
                        }
                        instance.status = WorkflowStatus::Failed;
                        self.storage
                            .save_workflow_instance(instance.clone())
                            .await?;
                        return Err(e.context("Task execution failed"));
                    }
                }
            }

            self.storage
                .save_workflow_instance(instance.clone())
                .await?;
        }

        let all_completed = instance
            .tasks
            .values()
            .all(|t| t.status == TaskStatus::Completed)
            && instance.verifier_states.values().all(|state| {
                matches!(
                    state.status,
                    VerifierStateStatus::Accepted | VerifierStateStatus::ExhaustedAccepted
                )
            });
        if all_completed {
            instance.status = WorkflowStatus::Completed;
            self.storage.save_workflow_instance(instance).await?;
        }

        Ok(())
    }

    fn compute_loop_slices(&self, def: &WorkflowDef) -> HashMap<String, Vec<String>> {
        let mut slices = HashMap::new();
        for verifier_task in def
            .tasks
            .iter()
            .filter(|task| task_verifier(task).is_some())
        {
            let verifier = task_verifier(verifier_task).unwrap();
            let rerun_start_task_id = verifier_rerun_start_task_id(verifier, &verifier_task.id);
            let from_start = self.reachable_from(def, &rerun_start_task_id);
            let to_verifier = self.can_reach_target_set(def, &verifier_task.id);
            let slice = def
                .tasks
                .iter()
                .filter_map(|task| {
                    (from_start.contains(&task.id) && to_verifier.contains(&task.id))
                        .then_some(task.id.clone())
                })
                .collect::<Vec<_>>();
            slices.insert(verifier_task.id.clone(), slice);
        }
        slices
    }

    fn reachable_from(&self, def: &WorkflowDef, start: &str) -> HashSet<String> {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        for binding in &def.data_bindings {
            adjacency
                .entry(binding.source_task_id.clone())
                .or_default()
                .push(binding.target_task_id.clone());
        }
        self.walk_graph(start, &adjacency)
    }

    fn can_reach_target_set(&self, def: &WorkflowDef, target: &str) -> HashSet<String> {
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for binding in &def.data_bindings {
            reverse
                .entry(binding.target_task_id.clone())
                .or_default()
                .push(binding.source_task_id.clone());
        }
        self.walk_graph(target, &reverse)
    }

    fn walk_graph(&self, start: &str, adjacency: &HashMap<String, Vec<String>>) -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut stack = vec![start.to_string()];
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(next) = adjacency.get(&current) {
                stack.extend(next.iter().cloned());
            }
        }
        seen
    }

    fn materialize_eligible_generations(
        &self,
        instance: &mut WorkflowInstance,
        def: &WorkflowDef,
        loop_slices: &HashMap<String, Vec<String>>,
    ) -> bool {
        let mut changed = false;
        for (verifier_task_id, slice) in loop_slices {
            if instance.verifier_states.contains_key(verifier_task_id) {
                continue;
            }
            let Some(verifier_task) = def.tasks.iter().find(|task| task.id == *verifier_task_id)
            else {
                continue;
            };
            let Some(verifier) = task_verifier(verifier_task) else {
                continue;
            };
            let Some(start_task) = def
                .tasks
                .iter()
                .find(|task| task.id == verifier_rerun_start_task_id(verifier, verifier_task_id))
            else {
                continue;
            };

            if !self.logical_task_ready_outside_slice(
                instance,
                def,
                start_task,
                slice.as_slice(),
                loop_slices,
            ) {
                continue;
            }

            instance.verifier_states.insert(
                verifier_task_id.clone(),
                VerifierGenerationState {
                    verifier_task_id: verifier_task_id.clone(),
                    rerun_start_task_id: verifier_rerun_start_task_id(verifier, verifier_task_id),
                    latest_generation: 1,
                    selected_generation: None,
                    feedback_history: vec![],
                    status: VerifierStateStatus::Running,
                    exit_reason: None,
                },
            );
            self.materialize_generation(instance, slice, 1);
            changed = true;
        }
        changed
    }

    fn logical_task_ready_outside_slice(
        &self,
        instance: &WorkflowInstance,
        def: &WorkflowDef,
        task_def: &crate::core::models::TaskDef,
        slice: &[String],
        loop_slices: &HashMap<String, Vec<String>>,
    ) -> bool {
        def.data_bindings
            .iter()
            .filter(|binding| binding.target_task_id == task_def.id)
            .all(|binding| {
                if slice.contains(&binding.source_task_id) {
                    return true;
                }
                self.resolve_source_attempt_id(instance, loop_slices, None, &binding.source_task_id)
                    .and_then(|source_id| instance.tasks.get(&source_id))
                    .is_some_and(|task| task.status == TaskStatus::Completed)
            })
    }

    fn materialize_generation(
        &self,
        instance: &mut WorkflowInstance,
        slice: &[String],
        generation_index: u32,
    ) {
        for task_def_id in slice {
            let attempt_id = Self::generation_attempt_id(task_def_id, generation_index);
            instance
                .tasks
                .entry(attempt_id.clone())
                .or_insert_with(|| TaskInstance {
                    task_def_id: task_def_id.clone(),
                    status: TaskStatus::Pending,
                    input_data: vec![],
                    output_data: None,
                    recorded_side_effects: vec![],
                    generation: TaskGenerationMetadata {
                        attempt_id,
                        original_task_def_id: task_def_id.clone(),
                        generation_index,
                    },
                    verifier_metadata: None,
                });
        }
    }

    fn generation_attempt_id(task_def_id: &str, generation_index: u32) -> String {
        format!("{task_def_id}[{generation_index}]")
    }

    fn resolve_inputs(
        &self,
        instance: &WorkflowInstance,
        def: &WorkflowDef,
        _task_id: &str,
        task_instance: &TaskInstance,
        task_def: &crate::core::models::TaskDef,
    ) -> Option<Vec<serde_json::Value>> {
        let loop_slices = self.compute_loop_slices(def);
        let target_generation = Some(&task_instance.generation);
        let mut inputs = Vec::new();

        for binding in def
            .data_bindings
            .iter()
            .filter(|binding| binding.target_task_id == task_def.id)
        {
            let source_id = self.resolve_source_attempt_id(
                instance,
                &loop_slices,
                target_generation,
                &binding.source_task_id,
            )?;
            let source_task = instance.tasks.get(&source_id)?;
            if source_task.status != TaskStatus::Completed {
                return None;
            }
            while inputs.len() <= binding.target_input_index {
                inputs.push(serde_json::Value::Null);
            }
            inputs[binding.target_input_index] = source_task
                .output_data
                .clone()
                .unwrap_or(serde_json::Value::Null);
        }

        Some(inputs)
    }

    fn resolve_source_attempt_id(
        &self,
        instance: &WorkflowInstance,
        loop_slices: &HashMap<String, Vec<String>>,
        target_generation: Option<&TaskGenerationMetadata>,
        source_task_id: &str,
    ) -> Option<String> {
        if let Some(generation) = target_generation {
            if let Some((_, slice)) = loop_slices
                .iter()
                .find(|(_, slice)| slice.contains(&generation.original_task_def_id))
            {
                if slice.contains(&source_task_id.to_string()) {
                    return Some(Self::generation_attempt_id(
                        source_task_id,
                        generation.generation_index,
                    ));
                }
            }
        }

        if let Some((verifier_id, _)) = loop_slices
            .iter()
            .find(|(_, slice)| slice.contains(&source_task_id.to_string()))
        {
            let selected_generation = instance
                .verifier_states
                .get(verifier_id)
                .and_then(|state| state.selected_generation)?;
            return Some(Self::generation_attempt_id(
                source_task_id,
                selected_generation,
            ));
        }

        Some(Self::generation_attempt_id(source_task_id, 1))
    }

    fn execution_metadata(
        &self,
        instance: &WorkflowInstance,
        def: &WorkflowDef,
        _task_id: &str,
        task_instance: &TaskInstance,
        _task_def: &crate::core::models::TaskDef,
    ) -> ExecutionMetadata {
        let generation = &task_instance.generation;
        let loop_slices = self.compute_loop_slices(def);
        let Some((verifier_id, _)) = loop_slices
            .iter()
            .find(|(_, slice)| slice.contains(&generation.original_task_def_id))
        else {
            return ExecutionMetadata::default();
        };
        let Some(verifier_task) = def.tasks.iter().find(|task| task.id == *verifier_id) else {
            return ExecutionMetadata::default();
        };
        let Some(verifier) = task_verifier(verifier_task) else {
            return ExecutionMetadata::default();
        };

        let feedback_history = instance
            .verifier_states
            .get(verifier_id)
            .map(|state| {
                state
                    .feedback_history
                    .iter()
                    .map(|entry| LoopFeedbackEntry {
                        generation: entry.generation_index,
                        feedback: entry.feedback.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let previous_output =
            generation
                .generation_index
                .checked_sub(1)
                .and_then(|previous_generation| {
                    let previous_attempt_id = Self::generation_attempt_id(
                        &generation.original_task_def_id,
                        previous_generation,
                    );
                    instance
                        .tasks
                        .get(&previous_attempt_id)
                        .and_then(|task| task.output_data.clone())
                });

        let loop_context = LoopExecutionContext {
            generation: generation.generation_index,
            max_iterations: verifier.max_iterations,
            feedback_history,
            previous_output,
        };

        ExecutionMetadata {
            loop_context: Some(loop_context),
        }
    }

    fn apply_verifier_result(
        &self,
        instance: &mut WorkflowInstance,
        def: &WorkflowDef,
        loop_slices: &HashMap<String, Vec<String>>,
        task_id: &str,
        task_output: &serde_json::Value,
        verifier_result: crate::core::models::VerifierExecutionResult,
    ) -> anyhow::Result<()> {
        let task = instance
            .tasks
            .get(task_id)
            .ok_or_else(|| anyhow::anyhow!("verifier task attempt {task_id} not found"))?;
        let generation = task.generation.generation_index;
        let verifier_task_id = task.task_def_id.clone();
        let verifier_task = def
            .tasks
            .iter()
            .find(|task| task.id == verifier_task_id)
            .ok_or_else(|| anyhow::anyhow!("verifier task definition missing"))?;
        let verifier = task_verifier(verifier_task)
            .ok_or_else(|| anyhow::anyhow!("task {verifier_task_id} has no verifier config"))?;
        let state = instance
            .verifier_states
            .get_mut(&verifier_task_id)
            .ok_or_else(|| anyhow::anyhow!("verifier state {verifier_task_id} missing"))?;

        match verifier_result.decision {
            VerifierDecision::Complete => {
                state.selected_generation = Some(generation);
                state.status = VerifierStateStatus::Accepted;
                state.exit_reason = Some("complete".to_string());
                if let Some(task) = instance.tasks.get_mut(task_id) {
                    task.verifier_metadata = Some(VerifierAttemptMetadata {
                        status: VerifierAttemptStatus::Accepted,
                        decision: Some(VerifierDecision::Complete),
                        feedback: verifier_result.feedback,
                        verifier_output: Some(verifier_result.output),
                        exit_reason: Some("complete".to_string()),
                    });
                }
            }
            VerifierDecision::Continue => {
                let feedback = verifier_result.feedback.clone().unwrap_or_default();
                if feedback.trim().is_empty() {
                    instance.status = WorkflowStatus::Failed;
                    if let Some(task) = instance.tasks.get_mut(task_id) {
                        task.status = TaskStatus::Failed;
                        task.verifier_metadata = Some(VerifierAttemptMetadata {
                            status: VerifierAttemptStatus::Invalid,
                            decision: Some(VerifierDecision::Continue),
                            feedback: verifier_result.feedback,
                            verifier_output: Some(verifier_result.output),
                            exit_reason: Some(
                                "continue decision requires non-empty feedback".to_string(),
                            ),
                        });
                    }
                    anyhow::bail!("Verifier continue decision requires non-empty feedback");
                }

                state
                    .feedback_history
                    .push(crate::core::models::VerifierFeedbackEntry {
                        generation_index: generation,
                        feedback: feedback.clone(),
                        verifier_output: verifier_result.output.clone(),
                    });

                if generation < verifier.max_iterations {
                    state.latest_generation = generation + 1;
                    state.status = VerifierStateStatus::Running;
                    if let Some(task) = instance.tasks.get_mut(task_id) {
                        task.verifier_metadata = Some(VerifierAttemptMetadata {
                            status: VerifierAttemptStatus::Rejected,
                            decision: Some(VerifierDecision::Continue),
                            feedback: Some(feedback),
                            verifier_output: Some(verifier_result.output),
                            exit_reason: None,
                        });
                    }
                    if let Some(slice) = loop_slices.get(&verifier_task_id) {
                        self.materialize_generation(instance, slice, generation + 1);
                    }
                    return Ok(());
                }

                state.latest_generation = generation;
                state.exit_reason = Some("max_iterations_exhausted".to_string());
                if verifier.on_exhausted_continue {
                    if task_output.is_null() && verifier_task.output_schema.is_none() {
                        state.status = VerifierStateStatus::Failed;
                        instance.status = WorkflowStatus::Failed;
                        if let Some(task) = instance.tasks.get_mut(task_id) {
                            task.status = TaskStatus::Failed;
                            task.verifier_metadata = Some(VerifierAttemptMetadata {
                                status: VerifierAttemptStatus::ExhaustedFailed,
                                decision: Some(VerifierDecision::Continue),
                                feedback: Some(feedback),
                                verifier_output: Some(verifier_result.output),
                                exit_reason: Some(
                                    "no schema-valid latest generation output".to_string(),
                                ),
                            });
                        }
                        anyhow::bail!(
                            "Verifier exhausted with continue policy but no schema-valid output"
                        );
                    }

                    state.selected_generation = Some(generation);
                    state.status = VerifierStateStatus::ExhaustedAccepted;
                    if let Some(task) = instance.tasks.get_mut(task_id) {
                        task.verifier_metadata = Some(VerifierAttemptMetadata {
                            status: VerifierAttemptStatus::ExhaustedAccepted,
                            decision: Some(VerifierDecision::Continue),
                            feedback: Some(feedback),
                            verifier_output: Some(verifier_result.output),
                            exit_reason: Some("max_iterations_exhausted".to_string()),
                        });
                    }
                } else {
                    state.status = VerifierStateStatus::ExhaustedFailed;
                    instance.status = WorkflowStatus::Failed;
                    if let Some(task) = instance.tasks.get_mut(task_id) {
                        task.status = TaskStatus::Failed;
                        task.verifier_metadata = Some(VerifierAttemptMetadata {
                            status: VerifierAttemptStatus::ExhaustedFailed,
                            decision: Some(VerifierDecision::Continue),
                            feedback: Some(feedback),
                            verifier_output: Some(verifier_result.output),
                            exit_reason: Some("max_iterations_exhausted".to_string()),
                        });
                    }
                    anyhow::bail!("Verifier exhausted iteration budget");
                }
            }
        }

        Ok(())
    }
}

fn task_verifier(task: &crate::core::models::TaskDef) -> Option<&VerifierControlConfig> {
    task.control.as_ref()?.verifier.as_ref()
}

fn effective_output_schema(
    task: &crate::core::models::TaskDef,
) -> Option<&crate::core::models::JsonSchema> {
    task.output_schema.as_ref()
}

fn validate_inputs(
    task: &crate::core::models::TaskDef,
    inputs: &[serde_json::Value],
) -> anyhow::Result<()> {
    for (index, schema) in task.input_schemas.iter().enumerate() {
        let Some(input) = inputs.get(index) else {
            anyhow::bail!("Task {} missing input at index {}", task.id, index);
        };
        let validator = jsonschema::validator_for(schema).map_err(|error| {
            anyhow::anyhow!(
                "Task {} input schema {} is invalid: {}",
                task.id,
                index,
                error
            )
        })?;
        if !validator.is_valid(input) {
            anyhow::bail!("Task {} input {} failed schema validation", task.id, index);
        }
    }

    Ok(())
}

fn verifier_rerun_start_task_id(
    verifier: &VerifierControlConfig,
    verifier_task_id: &str,
) -> String {
    verifier
        .rerun_from_task_id
        .clone()
        .unwrap_or_else(|| verifier_task_id.to_string())
}

fn verifier_result_from_output(
    output: &serde_json::Value,
) -> anyhow::Result<VerifierExecutionResult> {
    let object = output
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Verifier task output must be an object"))?;
    let decision = match object.get("decision").and_then(|value| value.as_str()) {
        Some("continue") => VerifierDecision::Continue,
        Some("complete") => VerifierDecision::Complete,
        _ => anyhow::bail!("Verifier task decision must be \"continue\" or \"complete\""),
    };
    let feedback = object
        .get("feedback")
        .and_then(|value| value.as_str())
        .map(str::to_string);

    if decision == VerifierDecision::Continue
        && feedback
            .as_deref()
            .is_none_or(|feedback| feedback.trim().is_empty())
    {
        anyhow::bail!("Verifier continue decision requires non-empty feedback");
    }

    Ok(VerifierExecutionResult {
        decision,
        feedback,
        output: output.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::core::models::*;
    use serde_json::Number;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_engine() -> WorkflowEngine {
        WorkflowEngine::new(
            Arc::new(MemoryStorage::new()),
            Arc::new(FakeExecutor::new()),
        )
    }

    fn task_def(id: &str, output_schema: serde_json::Value) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            kind: TaskTypeDef::ApiCall {
                url: "http://example.com".to_string(),
                method: "GET".to_string(),
            },
            control: None,
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: Some(output_schema),
            required_credentials: vec![],
        }
    }

    async fn setup(engine: &WorkflowEngine, def: WorkflowDef) -> String {
        let instance_id = "inst-1".to_string();
        let instance = WorkflowInstance {
            id: instance_id.clone(),
            workflow_def_id: def.id.clone(),
            status: WorkflowStatus::Pending,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        };
        engine.storage.save_workflow_def(def).await.unwrap();
        engine
            .storage
            .save_workflow_instance(instance)
            .await
            .unwrap();
        instance_id
    }

    fn agent_verifier_task(id: &str, rerun_from_task_id: Option<&str>) -> TaskDef {
        let mut task = task_def(id, json!({ "type": "object" }));
        task.kind = TaskTypeDef::Agent {
            model_id: "test/model".to_string(),
            provider_url: "".to_string(),
            prompt: "verify".to_string(),
            tools: vec![],
            skills: vec![],
            ask: false,
            schema_failure_retry_times: Number::from(0),
        };
        task.output_schema = None;
        task.control = Some(TaskControl {
            verifier: Some(VerifierControlConfig {
                max_iterations: 2,
                on_exhausted_continue: false,
                rerun_from_task_id: rerun_from_task_id.map(str::to_string),
            }),
        });
        task.output_schema = Some(verifier_decision_schema());
        task
    }

    #[test]
    fn test_verifier_without_rerun_from_task_id_self_reruns_only() {
        let engine = make_engine();
        let def = WorkflowDef {
            id: "def-self-rerun".to_string(),
            tasks: vec![
                task_def("task-a", json!({ "type": "object" })),
                agent_verifier_task("verify", None),
            ],
            data_bindings: vec![DataBinding {
                source_task_id: "task-a".to_string(),
                target_task_id: "verify".to_string(),
                target_input_index: 0,
            }],
        };

        let slices = engine.compute_loop_slices(&def);
        assert_eq!(slices["verify"], vec!["verify".to_string()]);
    }

    #[test]
    fn test_verifier_with_rerun_from_task_id_reruns_upstream_slice() {
        let engine = make_engine();
        let def = WorkflowDef {
            id: "def-upstream-rerun".to_string(),
            tasks: vec![
                task_def("task-a", json!({ "type": "object" })),
                task_def("task-b", json!({ "type": "object" })),
                agent_verifier_task("verify", Some("task-a")),
            ],
            data_bindings: vec![
                DataBinding {
                    source_task_id: "task-a".to_string(),
                    target_task_id: "task-b".to_string(),
                    target_input_index: 0,
                },
                DataBinding {
                    source_task_id: "task-b".to_string(),
                    target_task_id: "verify".to_string(),
                    target_input_index: 0,
                },
            ],
        };

        let slices = engine.compute_loop_slices(&def);
        assert_eq!(
            slices["verify"],
            vec![
                "task-a".to_string(),
                "task-b".to_string(),
                "verify".to_string()
            ]
        );
    }

    #[test]
    fn test_loop_execution_metadata_includes_feedback_history() {
        let engine = make_engine();
        let def = WorkflowDef {
            id: "def-loop-metadata".to_string(),
            tasks: vec![
                task_def("task-a", json!({ "type": "object" })),
                agent_verifier_task("verify", Some("task-a")),
            ],
            data_bindings: vec![DataBinding {
                source_task_id: "task-a".to_string(),
                target_task_id: "verify".to_string(),
                target_input_index: 0,
            }],
        };
        let task_instance = TaskInstance {
            task_def_id: "task-a".to_string(),
            status: TaskStatus::Pending,
            input_data: vec![],
            output_data: None,
            recorded_side_effects: vec![],
            generation: TaskGenerationMetadata {
                attempt_id: "task-a[2]".to_string(),
                original_task_def_id: "task-a".to_string(),
                generation_index: 2,
            },
            verifier_metadata: None,
        };
        let mut instance = WorkflowInstance {
            id: "inst-loop-metadata".to_string(),
            workflow_def_id: def.id.clone(),
            status: WorkflowStatus::Running,
            tasks: HashMap::from([
                (
                    "task-a[1]".to_string(),
                    TaskInstance {
                        task_def_id: "task-a".to_string(),
                        status: TaskStatus::Completed,
                        input_data: vec![],
                        output_data: Some(json!({ "draft": "first" })),
                        recorded_side_effects: vec![],
                        generation: TaskGenerationMetadata {
                            attempt_id: "task-a[1]".to_string(),
                            original_task_def_id: "task-a".to_string(),
                            generation_index: 1,
                        },
                        verifier_metadata: None,
                    },
                ),
                ("task-a[2]".to_string(), task_instance.clone()),
            ]),
            verifier_states: HashMap::new(),
        };
        instance.verifier_states.insert(
            "verify".to_string(),
            VerifierGenerationState {
                verifier_task_id: "verify".to_string(),
                rerun_start_task_id: "task-a".to_string(),
                latest_generation: 2,
                selected_generation: None,
                feedback_history: vec![VerifierFeedbackEntry {
                    generation_index: 1,
                    feedback: "Add citations.".to_string(),
                    verifier_output: json!({ "decision": "continue" }),
                }],
                status: VerifierStateStatus::Running,
                exit_reason: None,
            },
        );

        let metadata =
            engine.execution_metadata(&instance, &def, "task-a[2]", &task_instance, &def.tasks[0]);
        let loop_context = metadata.loop_context.unwrap();

        assert_eq!(
            loop_context.feedback_history,
            vec![LoopFeedbackEntry {
                generation: 1,
                feedback: "Add citations.".to_string(),
            }]
        );
        assert_eq!(
            loop_context.previous_output,
            Some(json!({ "draft": "first" }))
        );
    }

    /// A single task with no dependencies should run and complete the workflow.
    #[tokio::test]
    async fn test_single_task_workflow_completes() {
        let engine = make_engine();

        let def = WorkflowDef {
            id: "def-1".to_string(),
            tasks: vec![task_def("task-a", json!({ "type": "object" }))],
            data_bindings: vec![],
        };

        let instance_id = setup(&engine, def).await;
        engine
            .run_workflow_instance(instance_id.clone())
            .await
            .unwrap();

        let result = engine
            .storage
            .get_workflow_instance(&instance_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.status, WorkflowStatus::Completed);
        assert_eq!(result.tasks["task-a[1]"].status, TaskStatus::Completed);
        assert_eq!(result.tasks["task-a[1]"].task_def_id, "task-a");
        assert_eq!(result.tasks["task-a[1]"].generation.generation_index, 1);
    }

    /// Two independent tasks (A and B) feed into a third (C) via data bindings.
    /// C should only run after both A and B complete (Fan-In).
    #[tokio::test]
    async fn test_fan_in_workflow_completes_with_propagation() {
        let engine = make_engine();

        let def = WorkflowDef {
            id: "def-2".to_string(),
            tasks: vec![
                task_def("task-a", json!({ "type": "object" })),
                task_def("task-b", json!({ "type": "object" })),
                TaskDef {
                    id: "task-c".to_string(),
                    kind: TaskTypeDef::ApiCall {
                        url: "http://example.com".to_string(),
                        method: "POST".to_string(),
                    },
                    control: None,
                    timeout_secs: None,
                    input_schemas: vec![
                        json!({ "type": "object" }), // from task-a
                        json!({ "type": "object" }), // from task-b
                    ],
                    output_schema: Some(json!({ "type": "object" })),
                    required_credentials: vec![],
                },
            ],
            data_bindings: vec![
                DataBinding {
                    source_task_id: "task-a".to_string(),
                    target_task_id: "task-c".to_string(),
                    target_input_index: 0,
                },
                DataBinding {
                    source_task_id: "task-b".to_string(),
                    target_task_id: "task-c".to_string(),
                    target_input_index: 1,
                },
            ],
        };

        let instance_id = setup(&engine, def).await;
        engine
            .run_workflow_instance(instance_id.clone())
            .await
            .unwrap();

        let result = engine
            .storage
            .get_workflow_instance(&instance_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.status, WorkflowStatus::Completed);
        assert_eq!(result.tasks["task-a[1]"].status, TaskStatus::Completed);
        assert_eq!(result.tasks["task-b[1]"].status, TaskStatus::Completed);
        assert_eq!(result.tasks["task-c[1]"].status, TaskStatus::Completed);

        // task-c should have received propagated inputs at both index slots
        let task_c = &result.tasks["task-c[1]"];
        assert_eq!(task_c.input_data.len(), 2);
    }

    /// A task whose output fails schema validation should mark the workflow as Failed.
    #[tokio::test]
    async fn test_schema_validation_failure_marks_workflow_failed() {
        let engine = make_engine();

        // FakeExecutor cannot satisfy a `const` schema — it returns `{}` for unknown constructs,
        // which will always fail this constraint.
        let strict_schema = json!({
            "const": "only-this-value"
        });

        let def = WorkflowDef {
            id: "def-3".to_string(),
            tasks: vec![task_def("task-strict", strict_schema)],
            data_bindings: vec![],
        };

        let instance_id = setup(&engine, def).await;
        let run_result = engine.run_workflow_instance(instance_id.clone()).await;
        assert!(run_result.is_err());

        let instance = engine
            .storage
            .get_workflow_instance(&instance_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(instance.status, WorkflowStatus::Failed);
        assert_eq!(instance.tasks["task-strict[1]"].status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn test_input_schema_failure_marks_workflow_failed() {
        let engine = make_engine();
        let mut downstream = task_def("task-b", json!({ "type": "object" }));
        downstream.input_schemas = vec![json!({ "type": "string" })];
        let def = WorkflowDef {
            id: "def-input-schema".to_string(),
            tasks: vec![task_def("task-a", json!({ "type": "object" })), downstream],
            data_bindings: vec![DataBinding {
                source_task_id: "task-a".to_string(),
                target_task_id: "task-b".to_string(),
                target_input_index: 0,
            }],
        };

        let instance_id = setup(&engine, def).await;
        let run_result = engine.run_workflow_instance(instance_id.clone()).await;
        assert!(run_result.is_err());
        assert!(
            run_result
                .unwrap_err()
                .to_string()
                .contains("input 0 failed schema validation")
        );

        let instance = engine
            .storage
            .get_workflow_instance(&instance_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(instance.status, WorkflowStatus::Failed);
        assert_eq!(instance.tasks["task-b[1]"].status, TaskStatus::Failed);
    }

    /// After a successful run, get_workflow_status should return a report reflecting
    /// the completed state without exposing raw input/output data.
    #[tokio::test]
    async fn test_get_workflow_status_after_completion() {
        let engine = make_engine();

        let def = WorkflowDef {
            id: "def-status".to_string(),
            tasks: vec![
                task_def("task-a", json!({ "type": "object" })),
                task_def("task-b", json!({ "type": "object" })),
            ],
            data_bindings: vec![],
        };

        let instance_id = setup(&engine, def).await;
        engine
            .run_workflow_instance(instance_id.clone())
            .await
            .unwrap();

        let report = engine
            .get_workflow_status(&instance_id)
            .await
            .unwrap()
            .expect("report should be present");

        assert_eq!(report.instance_id, instance_id);
        assert_eq!(report.status, WorkflowStatus::Completed);
        assert_eq!(report.tasks.len(), 2);

        // Tasks are sorted by id, so task-a[1] comes first.
        assert_eq!(report.tasks[0].task_id, "task-a[1]");
        assert_eq!(report.tasks[0].task_def_id, "task-a");
        assert_eq!(report.tasks[0].status, TaskStatus::Completed);
        assert!(report.tasks[0].has_output);

        assert_eq!(report.tasks[1].task_id, "task-b[1]");
        assert_eq!(report.tasks[1].task_def_id, "task-b");
        assert_eq!(report.tasks[1].status, TaskStatus::Completed);
        assert!(report.tasks[1].has_output);
    }

    /// get_workflow_status should return None for an unknown instance id.
    #[tokio::test]
    async fn test_get_workflow_status_unknown_instance() {
        let engine = make_engine();
        let report = engine.get_workflow_status("does-not-exist").await.unwrap();
        assert!(report.is_none());
    }
}
