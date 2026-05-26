use crate::core::engine::WorkflowEngine;
use crate::core::function_resolution::resolve_task_function_ref;
use crate::core::models::{
    ExecutionMetadata, FunctionDef, TaskDef, TaskInstance, TaskStatus, VerifierControlConfig,
    WorkflowDef, WorkflowInstance, WorkflowList, WorkflowQueueStatus, WorkflowStatus,
    WorkflowStatusReport, WorkflowSummary, verifier_decision_schema,
};
use crate::ports::executor::ExecutorPort;
use crate::ports::storage::{StoragePort, TaskResult, TaskResultMetadata, WorkflowTaskResult};
use crate::ports::workflow_queue::WorkflowQueuePort;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info};

/// The application layer for the orchestrator.
/// It coordinates between the workflow engine, storage, and executors.
pub struct Orchestrator {
    engine: WorkflowEngine,
    storage: Arc<dyn StoragePort + Send + Sync>,
    executor: Arc<dyn ExecutorPort + Send + Sync>,
    workflow_queue: Arc<dyn WorkflowQueuePort + Send + Sync>,
}

impl Orchestrator {
    pub fn new(
        storage: Arc<dyn StoragePort + Send + Sync>,
        executor: Arc<dyn ExecutorPort + Send + Sync>,
        workflow_queue: Arc<dyn WorkflowQueuePort + Send + Sync>,
    ) -> Self {
        let engine = WorkflowEngine::new(storage.clone(), executor.clone());
        Self {
            engine,
            storage,
            executor,
            workflow_queue,
        }
    }

    /// Registers a new workflow definition.
    pub async fn create_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()> {
        let def = validate_and_normalize_workflow_def(def)?;
        self.storage.save_workflow_def(def).await
    }

    /// Retrieves a workflow definition by ID.
    pub async fn get_workflow_def(&self, id: &str) -> anyhow::Result<Option<WorkflowDef>> {
        self.storage.get_workflow_def(id).await
    }

    /// Registers a reusable function definition.
    pub async fn create_function_def(&self, def: FunctionDef) -> anyhow::Result<()> {
        self.storage.save_function_def(def).await
    }

    /// Deletes a reusable function definition.
    pub async fn delete_function_def(&self, id: &str) -> anyhow::Result<bool> {
        self.storage.delete_function_def(id).await
    }

    /// Finds a task in a registered workflow definition and executes it directly.
    pub async fn execute_workflow_task_isolated(
        &self,
        workflow_def_id: &str,
        task_id: &str,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<Option<crate::ports::executor::ExecutionResult>> {
        let Some(def) = self.storage.get_workflow_def(workflow_def_id).await? else {
            return Ok(None);
        };

        let Some(task) = def.tasks.into_iter().find(|task| task.id == task_id) else {
            return Ok(None);
        };

        self.execute_task_isolated(&task, inputs).await.map(Some)
    }

    /// Creates a new workflow instance.
    pub async fn create_workflow_instance(&self, instance: WorkflowInstance) -> anyhow::Result<()> {
        self.storage.save_workflow_instance(instance).await
    }

    /// Adds a workflow instance to the execution queue.
    pub async fn enqueue_workflow_instance(&self, instance_id: String) -> anyhow::Result<()> {
        self.workflow_queue.enqueue(instance_id).await
    }

    /// Returns queued and currently running workflow instance IDs.
    pub async fn get_queue_status(&self) -> anyhow::Result<WorkflowQueueStatus> {
        let pending = self.workflow_queue.pending_ids().await?;

        Ok(WorkflowQueueStatus { pending })
    }

    /// Removes a pending workflow instance from the queue.
    pub async fn remove_queued_workflow_instance(&self, instance_id: &str) -> anyhow::Result<bool> {
        self.workflow_queue.remove(instance_id).await
    }

    /// Removes all pending workflow instances from the queue.
    pub async fn purge_queued_workflow_instances(&self) -> anyhow::Result<Vec<String>> {
        self.workflow_queue.purge().await
    }

    /// Returns a status report for a workflow instance.
    pub async fn get_workflow_status(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<WorkflowStatusReport>> {
        self.engine.get_workflow_status(id).await
    }

    pub async fn list_workflows(
        &self,
        status: Option<WorkflowStatus>,
    ) -> anyhow::Result<WorkflowList> {
        let mut workflows: Vec<WorkflowSummary> = self
            .storage
            .list_workflow_instances()
            .await?
            .into_iter()
            .filter(|instance| {
                status
                    .as_ref()
                    .is_none_or(|status| instance.status == *status)
            })
            .map(|instance| WorkflowSummary {
                id: instance.id,
                workflow_def_id: instance.workflow_def_id,
                status: instance.status,
            })
            .collect();

        workflows.sort_by(|left, right| left.id.cmp(&right.id));

        Ok(WorkflowList { workflows })
    }

    pub async fn get_task_result(
        &self,
        workflow_instance_id: &str,
        task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        self.get_task_result_for_generation(workflow_instance_id, task_id, None)
            .await
    }

    pub async fn list_task_results(
        &self,
        workflow_instance_id: &str,
    ) -> anyhow::Result<Vec<WorkflowTaskResult>> {
        let instance = self
            .storage
            .get_workflow_instance(workflow_instance_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("workflow instance {workflow_instance_id} not found"))?;

        let mut tasks: Vec<WorkflowTaskResult> = instance
            .tasks
            .iter()
            .map(|(task_id, task)| WorkflowTaskResult {
                task_id: task_id.clone(),
                result: task_result_for_instance(
                    task_id,
                    task_id,
                    task,
                    should_include_task_result_metadata(task_id, task_id, task, None),
                ),
            })
            .collect();
        tasks.sort_by(|left, right| left.task_id.cmp(&right.task_id));

        Ok(tasks)
    }

    pub async fn get_task_result_for_generation(
        &self,
        workflow_instance_id: &str,
        task_id: &str,
        generation: Option<u32>,
    ) -> anyhow::Result<TaskResult> {
        let instance = self
            .storage
            .get_workflow_instance(workflow_instance_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("workflow instance {workflow_instance_id} not found"))?;

        let def = self
            .storage
            .get_workflow_def(&instance.workflow_def_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("workflow definition not found"))?;

        let resolved_attempt_id =
            resolve_task_lookup_attempt_id(&instance, &def, task_id, generation)?;

        let task = instance
            .tasks
            .get(&resolved_attempt_id)
            .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;

        Ok(task_result_for_instance(
            task_id,
            &resolved_attempt_id,
            task,
            should_include_task_result_metadata(task_id, &resolved_attempt_id, task, generation),
        ))
    }

    /// Starts or resumes execution of a workflow instance.
    pub async fn run_workflow(&self, instance_id: String) -> anyhow::Result<()> {
        self.engine.run_workflow_instance(instance_id).await
    }

    /// Continuously consumes queued workflow instances and runs up to `max_concurrent_workflows`.
    pub async fn run_scheduler(self: Arc<Self>, max_concurrent_workflows: usize) {
        let max_concurrent_workflows = max_concurrent_workflows.max(1);
        let permits = Arc::new(Semaphore::new(max_concurrent_workflows));
        info!(max_concurrent_workflows, "workflow scheduler started");

        loop {
            let permit = match Arc::clone(&permits).acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    error!(%error, "workflow scheduler semaphore closed");
                    break;
                }
            };

            let instance_id = match self.workflow_queue.dequeue().await {
                Ok(instance_id) => instance_id,
                Err(error) => {
                    error!(%error, "workflow scheduler failed to dequeue workflow instance");
                    drop(permit);
                    break;
                }
            };

            let orchestrator = Arc::clone(&self);
            tokio::spawn(async move {
                let workflow_instance_id = instance_id.clone();
                if let Err(error) = orchestrator.run_workflow(instance_id).await {
                    error!(
                        %workflow_instance_id,
                        error = ?error,
                        "workflow execution failed"
                    );
                }
                drop(permit);
            });
        }
    }

    /// Reconciles in-flight workflow state after an orchestrator restart.
    ///
    /// Storage is the source of truth. Any task left Running from a previous
    /// process is moved back to Pending so it can be dispatched again.
    pub async fn synchronize_startup_tasks(&self) -> anyhow::Result<usize> {
        let mut recovered = 0;

        for mut instance in self.storage.list_active_workflow_instances().await? {
            let mut changed = false;

            for task in instance.tasks.values_mut() {
                if task.status == TaskStatus::Running {
                    task.status = TaskStatus::Pending;
                    changed = true;
                }
            }

            if instance.status == WorkflowStatus::Running {
                instance.status = WorkflowStatus::Pending;
                changed = true;
            }

            if changed {
                self.storage.save_workflow_instance(instance).await?;
                recovered += 1;
            }
        }

        Ok(recovered)
    }

    /// Requeues all active workflow instances found in storage.
    pub async fn enqueue_active_workflow_instances(&self) -> anyhow::Result<usize> {
        let instances = self.storage.list_active_workflow_instances().await?;
        let count = instances.len();

        for instance in instances {
            self.enqueue_workflow_instance(instance.id).await?;
        }

        Ok(count)
    }

    /// Executes a single task in isolation, bypasses workflow orchestration.
    /// Useful for testing individual task types or executors.
    pub async fn execute_task_isolated(
        &self,
        task: &TaskDef,
        inputs: &[serde_json::Value],
    ) -> anyhow::Result<crate::ports::executor::ExecutionResult> {
        let task = self.resolve_task_function_ref(task).await?;
        self.executor
            .execute(&task, inputs, &ExecutionMetadata::default())
            .await
    }

    async fn resolve_task_function_ref(&self, task: &TaskDef) -> anyhow::Result<TaskDef> {
        resolve_task_function_ref(self.storage.as_ref(), task).await
    }
}

/// The result of a task execution, including its input, output or error, and optional metadata about how the result was obtained.
fn resolve_task_lookup_attempt_id(
    instance: &WorkflowInstance,
    def: &WorkflowDef,
    requested_task_id: &str,
    requested_generation: Option<u32>,
) -> anyhow::Result<String> {
    let normalized_task_id = requested_task_id.to_ascii_lowercase();

    if let Some(generation) = requested_generation {
        if generation == 0 {
            anyhow::bail!("generation must be positive");
        }
        return Ok(generation_attempt_id(&normalized_task_id, generation));
    }

    if instance.tasks.contains_key(requested_task_id) {
        return Ok(requested_task_id.to_string());
    }

    if normalized_task_id != requested_task_id && instance.tasks.contains_key(&normalized_task_id) {
        return Ok(normalized_task_id);
    }

    let loop_slices = compute_loop_slices(def);

    if let Some((verifier_task_id, _)) = loop_slices
        .iter()
        .find(|(_, slice)| slice.contains(&normalized_task_id))
    {
        let state = instance
            .verifier_states
            .get(verifier_task_id)
            .ok_or_else(|| anyhow::anyhow!("task {requested_task_id} not found"))?;
        let generation = state.selected_generation.unwrap_or(state.latest_generation);
        return Ok(generation_attempt_id(&normalized_task_id, generation));
    }

    Ok(generation_attempt_id(&normalized_task_id, 1))
}

fn should_include_task_result_metadata(
    requested_task_id: &str,
    resolved_attempt_id: &str,
    task: &TaskInstance,
    requested_generation: Option<u32>,
) -> bool {
    requested_generation.is_some()
        || requested_task_id != resolved_attempt_id
        || task.generation.generation_index > 0
        || task.verifier_metadata.is_some()
}

fn task_result_for_instance(
    requested_task_id: &str,
    resolved_attempt_id: &str,
    task: &TaskInstance,
    include_metadata: bool,
) -> TaskResult {
    let metadata = include_metadata.then(|| TaskResultMetadata {
        requested_task_id: requested_task_id.to_string(),
        resolved_attempt_id: resolved_attempt_id.to_string(),
        generation: Some(task.generation.clone()),
        verifier_metadata: task.verifier_metadata.clone(),
    });

    match (&task.status, metadata) {
        (TaskStatus::Completed, Some(metadata)) => TaskResult::SuccessWithMetadata {
            input: task.input_data.clone(),
            output: task.output_data.clone().unwrap_or(serde_json::Value::Null),
            metadata,
        },
        (TaskStatus::Completed, None) => TaskResult::Success {
            input: task.input_data.clone(),
            output: task.output_data.clone().unwrap_or(serde_json::Value::Null),
        },
        (TaskStatus::Failed, Some(metadata)) => TaskResult::FailureWithMetadata {
            input: task.input_data.clone(),
            error_message: "task failed".to_string(),
            metadata,
        },
        (TaskStatus::Failed, None) => TaskResult::Failure {
            input: task.input_data.clone(),
            error_message: "task failed".to_string(),
        },
        (TaskStatus::Pending, Some(metadata)) => TaskResult::PendingWithMetadata {
            input: task.input_data.clone(),
            metadata,
        },
        (TaskStatus::Pending, None) => TaskResult::Pending {
            input: task.input_data.clone(),
        },
        (TaskStatus::Running | TaskStatus::InputNeeded { .. }, Some(metadata)) => {
            TaskResult::RunningWithMetadata {
                input: task.input_data.clone(),
                metadata,
            }
        }
        (TaskStatus::Running | TaskStatus::InputNeeded { .. }, None) => TaskResult::Running {
            input: task.input_data.clone(),
        },
    }
}

// Returns a map of verifier task ID to the set of task IDs that are in its loop slice.
fn compute_loop_slices(def: &WorkflowDef) -> HashMap<String, Vec<String>> {
    let mut slices = HashMap::new();

    for verifier_task in def
        .tasks
        .iter()
        .filter(|task| get_task_verifier_config(task).is_some())
    {
        let verifier_config = get_task_verifier_config(verifier_task).unwrap();
        let rerun_start_task_id = verifier_rerun_start_task_id(verifier_config, &verifier_task.id);
        let from_start = reachable_from(def, &rerun_start_task_id);
        let to_verifier = can_reach_target_set(def, &verifier_task.id);
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

fn reachable_from(def: &WorkflowDef, start: &str) -> HashSet<String> {
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for binding in &def.data_bindings {
        adjacency
            .entry(binding.source_task_id.clone())
            .or_default()
            .push(binding.target_task_id.clone());
    }
    walk_graph(start, &adjacency)
}

fn can_reach_target_set(def: &WorkflowDef, target: &str) -> HashSet<String> {
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for binding in &def.data_bindings {
        reverse
            .entry(binding.target_task_id.clone())
            .or_default()
            .push(binding.source_task_id.clone());
    }
    walk_graph(target, &reverse)
}

fn walk_graph(start: &str, adjacency: &HashMap<String, Vec<String>>) -> HashSet<String> {
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

fn generation_attempt_id(task_def_id: &str, generation_index: u32) -> String {
    format!("{task_def_id}[{generation_index}]")
}

fn get_task_verifier_config(task: &TaskDef) -> Option<&VerifierControlConfig> {
    task.control.as_ref()?.verifier.as_ref()
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

fn validate_and_normalize_workflow_def(mut def: WorkflowDef) -> anyhow::Result<WorkflowDef> {
    validate_ascii_alphanumeric_id("workflow", &def.id)?;
    def.id = def.id.to_ascii_lowercase();

    let mut original_to_normalized = HashMap::new();
    let mut normalized_task_ids = HashSet::new();

    for task in &mut def.tasks {
        validate_ascii_alphanumeric_id("task", &task.id)?;
        let normalized = task.id.to_ascii_lowercase();
        if !normalized_task_ids.insert(normalized.clone()) {
            anyhow::bail!("duplicate task id after normalization: {normalized}");
        }
        original_to_normalized.insert(task.id.clone(), normalized.clone());
        task.id = normalized;
    }

    for binding in &mut def.data_bindings {
        binding.source_task_id = normalize_task_reference(
            &original_to_normalized,
            &binding.source_task_id,
            "data binding source",
        )?;
        binding.target_task_id = normalize_task_reference(
            &original_to_normalized,
            &binding.target_task_id,
            "data binding target",
        )?;
    }

    for task in &mut def.tasks {
        if get_task_verifier_config(task).is_some() && task.output_schema.is_some() {
            anyhow::bail!(
                "task {} declares control.verifier and must not declare output_schema",
                task.id
            );
        }
        if let Some(verifier) = task
            .control
            .as_mut()
            .and_then(|control| control.verifier.as_mut())
        {
            if verifier.max_iterations == 0 {
                anyhow::bail!("task {} verifier max_iterations must be positive", task.id);
            }
            if let Some(rerun_from_task_id) = verifier.rerun_from_task_id.as_mut() {
                *rerun_from_task_id = normalize_task_reference(
                    &original_to_normalized,
                    rerun_from_task_id,
                    "verifier rerun_from_task_id",
                )?;
            }
            task.output_schema = Some(verifier_decision_schema());
        }
    }

    if has_data_binding_cycle(&def) {
        anyhow::bail!("workflow data bindings contain a cycle");
    }

    for task in &def.tasks {
        if let Some(verifier) = get_task_verifier_config(task) {
            let Some(rerun_from_task_id) = &verifier.rerun_from_task_id else {
                continue;
            };
            if rerun_from_task_id != &task.id
                && !is_upstream_ancestor(&def, rerun_from_task_id, &task.id)
            {
                anyhow::bail!(
                    "task {} verifier rerun task {} is not an upstream ancestor",
                    task.id,
                    rerun_from_task_id
                );
            }
        }
    }

    Ok(def)
}

fn validate_ascii_alphanumeric_id(kind: &str, id: &str) -> anyhow::Result<()> {
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        anyhow::bail!("{kind} id {id:?} must contain only ASCII alphanumeric characters");
    }
    Ok(())
}

fn normalize_task_reference(
    original_to_normalized: &HashMap<String, String>,
    id: &str,
    field: &str,
) -> anyhow::Result<String> {
    validate_ascii_alphanumeric_id(field, id)?;
    original_to_normalized
        .get(id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{field} references unknown task id {id}"))
}

fn has_data_binding_cycle(def: &WorkflowDef) -> bool {
    let mut in_degree: HashMap<String, usize> = def
        .tasks
        .iter()
        .map(|task| (task.id.clone(), 0usize))
        .collect();
    let mut adjacency: HashMap<String, Vec<String>> = def
        .tasks
        .iter()
        .map(|task| (task.id.clone(), Vec::new()))
        .collect();

    for binding in &def.data_bindings {
        *in_degree.entry(binding.target_task_id.clone()).or_insert(0) += 1;
        adjacency
            .entry(binding.source_task_id.clone())
            .or_default()
            .push(binding.target_task_id.clone());
    }

    let mut queue = in_degree
        .iter()
        .filter_map(|(node, degree)| (*degree == 0).then_some(node.clone()))
        .collect::<VecDeque<_>>();
    let mut visited = 0usize;

    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(neighbors) = adjacency.get(&node) {
            for neighbor in neighbors {
                if let Some(degree) = in_degree.get_mut(neighbor) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    visited != def.tasks.len()
}

fn is_upstream_ancestor(def: &WorkflowDef, ancestor: &str, task_id: &str) -> bool {
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for binding in &def.data_bindings {
        reverse
            .entry(binding.target_task_id.clone())
            .or_default()
            .push(binding.source_task_id.clone());
    }

    let mut stack = vec![task_id.to_string()];
    let mut seen = HashSet::new();
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some(upstream) = reverse.get(&current) {
            for source in upstream {
                if source == ancestor {
                    return true;
                }
                stack.push(source.clone());
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake_executor::FakeExecutor;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::adapters::memory_workflow_queue::MemoryWorkflowQueue;
    use crate::core::models::{FunctionDef, FunctionTaskDef, TaskTypeDef};
    use crate::ports::executor::ExecutionResult;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{Duration, sleep};

    fn orchestrator() -> Orchestrator {
        Orchestrator::new(
            Arc::new(MemoryStorage::new()),
            Arc::new(FakeExecutor::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        )
    }

    struct CountingExecutor {
        active: AtomicUsize,
        max_active: AtomicUsize,
        delay: Duration,
    }

    impl CountingExecutor {
        fn new(delay: Duration) -> Self {
            Self {
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
                delay,
            }
        }

        fn max_active(&self) -> usize {
            self.max_active.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ExecutorPort for CountingExecutor {
        async fn execute(
            &self,
            _task: &TaskDef,
            _inputs: &[serde_json::Value],
            _metadata: &ExecutionMetadata,
        ) -> anyhow::Result<ExecutionResult> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            sleep(self.delay).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(ExecutionResult::Success(json!({})))
        }
    }

    fn task(id: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            kind: TaskTypeDef::Function(FunctionTaskDef::Inline {
                dependencies: vec![],
                code: "export default async function run() { return {}; }".to_string(),
            }),
            control: None,
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: Some(json!({
                "type": "object",
                "required": ["ok"],
                "properties": {
                    "ok": { "type": "boolean" }
                }
            })),
            required_credentials: vec![],
        }
    }

    fn function_ref_task(id: &str, reference: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            kind: TaskTypeDef::Function(FunctionTaskDef::Ref {
                reference: reference.to_string(),
            }),
            control: None,
            timeout_secs: None,
            input_schemas: vec![],
            output_schema: Some(json!({
                "type": "object",
                "required": ["ok"],
                "properties": {
                    "ok": { "type": "boolean" }
                }
            })),
            required_credentials: vec![],
        }
    }

    fn workflow(id: &str, tasks: Vec<TaskDef>) -> WorkflowDef {
        WorkflowDef {
            id: id.to_string(),
            tasks,
            data_bindings: vec![],
        }
    }

    fn workflow_instance(id: &str, workflow_def_id: &str) -> WorkflowInstance {
        WorkflowInstance {
            id: id.to_string(),
            workflow_def_id: workflow_def_id.to_string(),
            status: WorkflowStatus::Pending,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_finds_registered_task() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(workflow("workflow1", vec![task("taska")]))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("workflow1", "taska", &[])
            .await
            .unwrap();

        assert_eq!(
            result,
            Some(ExecutionResult::Success(json!({ "ok": false })))
        );
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_scopes_task_lookup_to_workflow_def() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(workflow("workflow1", vec![task("taska")]))
            .await
            .unwrap();
        orchestrator
            .create_workflow_def(workflow("workflow2", vec![task("taska")]))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("workflow2", "taska", &[])
            .await
            .unwrap();

        assert_eq!(
            result,
            Some(ExecutionResult::Success(json!({ "ok": false })))
        );
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_resolves_registered_function_ref() {
        let orchestrator = orchestrator();
        orchestrator
            .create_function_def(FunctionDef {
                id: "functiona".to_string(),
                dependencies: vec![],
                code: "export default async function run() { return {}; }".to_string(),
            })
            .await
            .unwrap();
        orchestrator
            .create_workflow_def(workflow(
                "workflow1",
                vec![function_ref_task("taska", "functiona")],
            ))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("workflow1", "taska", &[])
            .await
            .unwrap();

        assert_eq!(
            result,
            Some(ExecutionResult::Success(json!({ "ok": false })))
        );
    }

    #[tokio::test]
    async fn execute_workflow_task_isolated_errors_for_missing_function_ref() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(workflow(
                "workflow1",
                vec![function_ref_task("taska", "missingfunction")],
            ))
            .await
            .unwrap();

        let error = orchestrator
            .execute_workflow_task_isolated("workflow1", "taska", &[])
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Function definition not found: missingfunction")
        );
    }

    #[tokio::test]
    async fn scheduler_limits_concurrent_workflow_execution() {
        let storage = Arc::new(MemoryStorage::new());
        let executor = Arc::new(CountingExecutor::new(Duration::from_millis(50)));
        let queue = Arc::new(MemoryWorkflowQueue::new(10));
        let orchestrator = Arc::new(Orchestrator::new(storage.clone(), executor.clone(), queue));
        let scheduler = tokio::spawn(orchestrator.clone().run_scheduler(2));

        for id in ["workflow-1", "workflow-2", "workflow-3"] {
            storage
                .save_workflow_def(workflow(
                    id,
                    vec![TaskDef {
                        output_schema: None,
                        ..task("task-a")
                    }],
                ))
                .await
                .unwrap();
            storage
                .save_workflow_instance(workflow_instance(id, id))
                .await
                .unwrap();
            orchestrator
                .enqueue_workflow_instance(id.to_string())
                .await
                .unwrap();
        }

        for _ in 0..20 {
            let mut completed = 0;
            for id in ["workflow-1", "workflow-2", "workflow-3"] {
                let instance = storage.get_workflow_instance(id).await.unwrap().unwrap();
                if instance.status == WorkflowStatus::Completed {
                    completed += 1;
                }
            }
            if completed == 3 {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }

        assert_eq!(executor.max_active(), 2);
        scheduler.abort();
    }

    #[tokio::test]
    async fn isolated_workflow_task_execution_does_not_require_scheduler() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(workflow("workflow1", vec![task("taska")]))
            .await
            .unwrap();

        let result = orchestrator
            .execute_workflow_task_isolated("workflow1", "taska", &[])
            .await
            .unwrap();

        assert!(matches!(result, Some(ExecutionResult::Success(_))));
    }

    #[tokio::test]
    async fn create_workflow_def_accepts_missing_input_schemas() {
        let orchestrator = orchestrator();
        let workflow_def: WorkflowDef = serde_json::from_value(json!({
            "id": "workflow1",
            "tasks": [
                {
                    "id": "taska",
                    "kind": {
                        "Function": {
                            "dependencies": [],
                            "code": "export default async function run() { return {}; }"
                        }
                    },
                    "output_schema": {
                        "type": "object"
                    },
                    "required_credentials": []
                }
            ],
            "data_bindings": []
        }))
        .unwrap();

        orchestrator
            .create_workflow_def(workflow_def)
            .await
            .unwrap();

        let stored = orchestrator
            .get_workflow_def("workflow1")
            .await
            .unwrap()
            .unwrap();
        assert!(stored.tasks[0].input_schemas.is_empty());
    }

    #[tokio::test]
    async fn get_task_result_resolves_logical_task_id_to_generation_one() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(workflow("workflow1", vec![task("taska")]))
            .await
            .unwrap();
        orchestrator
            .create_workflow_instance(workflow_instance("instance1", "workflow1"))
            .await
            .unwrap();

        orchestrator
            .run_workflow("instance1".to_string())
            .await
            .unwrap();

        match orchestrator
            .get_task_result("instance1", "taska")
            .await
            .unwrap()
        {
            TaskResult::SuccessWithMetadata {
                input,
                output,
                metadata,
            } => {
                assert_eq!(input, Vec::<serde_json::Value>::new());
                assert_eq!(output, json!({ "ok": false }));
                assert_eq!(metadata.requested_task_id, "taska");
                assert_eq!(metadata.resolved_attempt_id, "taska[1]");
                assert_eq!(metadata.generation.unwrap().generation_index, 1);
            }
            result => panic!("expected success with metadata, got {result:?}"),
        }
    }

    #[tokio::test]
    async fn list_task_results_returns_materialized_attempts() {
        let orchestrator = orchestrator();
        orchestrator
            .create_workflow_def(workflow("workflow1", vec![task("taska")]))
            .await
            .unwrap();
        orchestrator
            .create_workflow_instance(workflow_instance("instance1", "workflow1"))
            .await
            .unwrap();

        orchestrator
            .run_workflow("instance1".to_string())
            .await
            .unwrap();

        let tasks = orchestrator.list_task_results("instance1").await.unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "taska[1]");
        match &tasks[0].result {
            TaskResult::SuccessWithMetadata {
                input,
                output,
                metadata,
            } => {
                assert_eq!(input, &Vec::<serde_json::Value>::new());
                assert_eq!(output, &json!({ "ok": false }));
                assert_eq!(metadata.requested_task_id, "taska[1]");
                assert_eq!(metadata.resolved_attempt_id, "taska[1]");
                assert_eq!(
                    metadata.generation.as_ref().unwrap().original_task_def_id,
                    "taska"
                );
            }
            result => panic!("expected success with metadata, got {result:?}"),
        }
    }

    #[tokio::test]
    async fn verifier_control_injects_decision_schema() {
        let orchestrator = orchestrator();
        let mut verifier = task("verify");
        verifier.output_schema = None;
        verifier.control = Some(crate::core::models::TaskControl {
            verifier: Some(crate::core::models::VerifierControlConfig {
                max_iterations: 2,
                on_exhausted_continue: false,
                rerun_from_task_id: None,
            }),
        });

        orchestrator
            .create_workflow_def(workflow("workflow1", vec![verifier]))
            .await
            .unwrap();

        let def = orchestrator
            .get_workflow_def("workflow1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(def.tasks[0].output_schema, Some(verifier_decision_schema()));
    }

    #[tokio::test]
    async fn verifier_control_rejects_user_output_schema() {
        let orchestrator = orchestrator();
        let mut verifier = task("verify");
        verifier.control = Some(crate::core::models::TaskControl {
            verifier: Some(crate::core::models::VerifierControlConfig {
                max_iterations: 2,
                on_exhausted_continue: false,
                rerun_from_task_id: None,
            }),
        });

        let error = orchestrator
            .create_workflow_def(workflow("workflow1", vec![verifier]))
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("control.verifier and must not declare output_schema")
        );
    }

    #[tokio::test]
    async fn queue_status_lists_pending_workflows() {
        let storage = Arc::new(MemoryStorage::new());
        let queue = Arc::new(MemoryWorkflowQueue::new(10));
        let orchestrator = Orchestrator::new(storage.clone(), Arc::new(FakeExecutor::new()), queue);

        let mut running = workflow_instance("running-workflow", "workflow-1");
        running.status = WorkflowStatus::Running;
        storage.save_workflow_instance(running).await.unwrap();

        orchestrator
            .enqueue_workflow_instance("pending-workflow".to_string())
            .await
            .unwrap();

        assert_eq!(
            orchestrator.get_queue_status().await.unwrap(),
            WorkflowQueueStatus {
                pending: vec!["pending-workflow".to_string()],
            }
        );
    }

    #[tokio::test]
    async fn remove_and_purge_affect_pending_queue_only() {
        let orchestrator = orchestrator();

        orchestrator
            .enqueue_workflow_instance("workflow-1".to_string())
            .await
            .unwrap();
        orchestrator
            .enqueue_workflow_instance("workflow-2".to_string())
            .await
            .unwrap();

        assert!(
            orchestrator
                .remove_queued_workflow_instance("workflow-1")
                .await
                .unwrap()
        );
        assert_eq!(
            orchestrator
                .purge_queued_workflow_instances()
                .await
                .unwrap(),
            vec!["workflow-2".to_string()]
        );
        assert!(
            orchestrator
                .get_queue_status()
                .await
                .unwrap()
                .pending
                .is_empty()
        );
    }

    #[tokio::test]
    async fn list_workflows_filters_by_status() {
        let storage = Arc::new(MemoryStorage::new());
        let orchestrator = Orchestrator::new(
            storage.clone(),
            Arc::new(FakeExecutor::new()),
            Arc::new(MemoryWorkflowQueue::new(10)),
        );

        let mut completed = workflow_instance("completed-workflow", "workflow-1");
        completed.status = WorkflowStatus::Completed;
        let mut running = workflow_instance("running-workflow", "workflow-1");
        running.status = WorkflowStatus::Running;
        storage.save_workflow_instance(completed).await.unwrap();
        storage.save_workflow_instance(running).await.unwrap();

        assert_eq!(
            orchestrator
                .list_workflows(Some(WorkflowStatus::Running))
                .await
                .unwrap(),
            WorkflowList {
                workflows: vec![WorkflowSummary {
                    id: "running-workflow".to_string(),
                    workflow_def_id: "workflow-1".to_string(),
                    status: WorkflowStatus::Running,
                }],
            }
        );
    }
}
