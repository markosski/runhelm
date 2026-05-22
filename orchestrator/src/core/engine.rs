use crate::core::function_resolution::resolve_task_function_ref;
use crate::core::models::{
    ExecutionMetadata, LoopExecutionContext, TaskGenerationMetadata, TaskInstance, TaskStatus,
    TaskStatusReport, VerifierAttemptMetadata, VerifierAttemptStatus, VerifierDecision,
    VerifierExecutionContext, VerifierGenerationState, VerifierStateStatus, VerifierStatusReport,
    WorkflowDef, WorkflowInstance, WorkflowStatus, WorkflowStatusReport,
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
                generation: t.generation.clone(),
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
        let loop_task_to_verifier = self.loop_task_to_verifier(&loop_slices);

        // Initialize tasks if not already done
        if instance.tasks.is_empty() {
            for task_def in &def.tasks {
                if loop_task_to_verifier.contains_key(&task_def.id) {
                    continue;
                }
                instance.tasks.insert(
                    task_def.id.clone(),
                    TaskInstance {
                        task_def_id: task_def.id.clone(),
                        status: TaskStatus::Pending,
                        input_data: vec![], // Empty until upstream dependencies propagate data
                        output_data: None,
                        recorded_side_effects: vec![],
                        generation: None,
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
                        let (output, verifier_result) = match result {
                            ExecutionResult::Success(output) => (output, None),
                            ExecutionResult::SuccessWithVerifier { output, verifier } => {
                                (output, Some(verifier))
                            }
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
                        let schema_ok = match &task_def.output_schema {
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
                                if task_def.output_schema.is_some() {
                                    task.output_data = Some(output.clone());
                                }
                            }
                            if task_def.verifier.is_some() {
                                let Some(verifier_result) = verifier_result else {
                                    if let Some(task) = instance.tasks.get_mut(&task_id) {
                                        task.status = TaskStatus::Failed;
                                        task.verifier_metadata = Some(VerifierAttemptMetadata {
                                            status: VerifierAttemptStatus::Invalid,
                                            decision: None,
                                            feedback: None,
                                            verifier_output: None,
                                            exit_reason: Some(
                                                "verifier task completed without verifier result"
                                                    .to_string(),
                                            ),
                                        });
                                    }
                                    instance.status = WorkflowStatus::Failed;
                                    self.storage
                                        .save_workflow_instance(instance.clone())
                                        .await?;
                                    anyhow::bail!(
                                        "Verifier task completed without verifier result"
                                    );
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
        for verifier_task in def.tasks.iter().filter(|task| task.verifier.is_some()) {
            let verifier = verifier_task.verifier.as_ref().unwrap();
            let from_start = self.reachable_from(def, &verifier.on_failure_rerun_task);
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

    fn loop_task_to_verifier(
        &self,
        loop_slices: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, String> {
        let mut task_to_verifier = HashMap::new();
        for (verifier_id, slice) in loop_slices {
            for task_id in slice {
                task_to_verifier.insert(task_id.clone(), verifier_id.clone());
            }
        }
        task_to_verifier
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
            let Some(verifier) = verifier_task.verifier.as_ref() else {
                continue;
            };
            let Some(start_task) = def
                .tasks
                .iter()
                .find(|task| task.id == verifier.on_failure_rerun_task)
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
                    rerun_start_task_id: verifier.on_failure_rerun_task.clone(),
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
                    generation: Some(TaskGenerationMetadata {
                        attempt_id,
                        original_task_def_id: task_def_id.clone(),
                        generation_index,
                    }),
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
        let target_generation = task_instance.generation.as_ref();
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

        Some(source_task_id.to_string())
    }

    fn execution_metadata(
        &self,
        instance: &WorkflowInstance,
        def: &WorkflowDef,
        _task_id: &str,
        task_instance: &TaskInstance,
        task_def: &crate::core::models::TaskDef,
    ) -> ExecutionMetadata {
        let Some(generation) = task_instance.generation.as_ref() else {
            return ExecutionMetadata::default();
        };
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
        let Some(verifier) = verifier_task.verifier.as_ref() else {
            return ExecutionMetadata::default();
        };
        let feedback_history = instance
            .verifier_states
            .get(verifier_id)
            .map(|state| {
                state
                    .feedback_history
                    .iter()
                    .map(|entry| entry.feedback.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let latest_feedback = feedback_history.last().cloned();
        let loop_context = LoopExecutionContext {
            generation: generation.generation_index,
            max_iterations: verifier.max_iterations,
            latest_feedback,
            feedback_history: feedback_history.clone(),
        };

        let verifier_context = task_def.verifier.as_ref().map(|_| {
            let mut upstream_context = HashMap::new();
            for binding in def
                .data_bindings
                .iter()
                .filter(|binding| binding.target_task_id == task_def.id)
            {
                if let Some(source_id) = self.resolve_source_attempt_id(
                    instance,
                    &loop_slices,
                    Some(generation),
                    &binding.source_task_id,
                ) {
                    if let Some(source_task) = instance.tasks.get(&source_id) {
                        if source_task.status == TaskStatus::Completed {
                            upstream_context.insert(
                                binding.source_task_id.clone(),
                                source_task
                                    .output_data
                                    .clone()
                                    .unwrap_or(serde_json::Value::Null),
                            );
                        }
                    }
                }
            }

            VerifierExecutionContext {
                output: serde_json::Value::Null,
                generation: generation.generation_index,
                max_iterations: verifier.max_iterations,
                feedback_history,
                upstream_context,
            }
        });

        ExecutionMetadata {
            loop_context: Some(loop_context),
            verifier_context,
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
        let generation = task
            .generation
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("verifier task {task_id} is not materialized"))?
            .generation_index;
        let verifier_task_id = task.task_def_id.clone();
        let verifier_task = def
            .tasks
            .iter()
            .find(|task| task.id == verifier_task_id)
            .ok_or_else(|| anyhow::anyhow!("verifier task definition missing"))?;
        let verifier = verifier_task
            .verifier
            .as_ref()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::core::models::*;
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
            verifier: None,
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: Some(output_schema),
            expected_side_effects: vec![],
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
        assert_eq!(result.tasks["task-a"].status, TaskStatus::Completed);
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
                    verifier: None,
                    timeout_secs: None,
                    input_schemas: vec![
                        json!({ "type": "object" }), // from task-a
                        json!({ "type": "object" }), // from task-b
                    ],
                    output_schema: Some(json!({ "type": "object" })),
                    expected_side_effects: vec![],
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
        assert_eq!(result.tasks["task-a"].status, TaskStatus::Completed);
        assert_eq!(result.tasks["task-b"].status, TaskStatus::Completed);
        assert_eq!(result.tasks["task-c"].status, TaskStatus::Completed);

        // task-c should have received propagated inputs at both index slots
        let task_c = &result.tasks["task-c"];
        assert_eq!(task_c.input_data.len(), 2);
    }

    /// A task whose output fails schema validation should mark the workflow as Failed.
    #[tokio::test]
    async fn test_schema_validation_failure_marks_workflow_failed() {
        let engine = make_engine();

        // FakeExecutor cannot satisfy an `enum` schema — it returns `{}` for unknown constructs,
        // which will always fail a single-value enum constraint.
        let strict_schema = json!({
            "enum": ["only-this-value"]
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
        assert_eq!(instance.tasks["task-strict"].status, TaskStatus::Failed);
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

        // Tasks are sorted by id, so task-a comes first.
        assert_eq!(report.tasks[0].task_id, "task-a");
        assert_eq!(report.tasks[0].status, TaskStatus::Completed);
        assert!(report.tasks[0].has_output);

        assert_eq!(report.tasks[1].task_id, "task-b");
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
