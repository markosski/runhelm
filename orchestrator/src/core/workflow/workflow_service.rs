use crate::api::models::{WorkflowList, WorkflowSummary};
use crate::core::models::{
    TaskDef, TaskInstance, TaskStatus, VerifierControlConfig, verifier_decision_schema,
};
use crate::core::workflow::models::{WorkflowDef, WorkflowInstance, WorkflowStatus};
use crate::ports::storage::{StoragePort, TaskResult, TaskResultMetadata, WorkflowTaskResult};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct WorkflowService {
    storage: Arc<dyn StoragePort + Send + Sync>,
}

impl WorkflowService {
    pub fn new(storage: Arc<dyn StoragePort + Send + Sync>) -> Self {
        Self { storage }
    }

    pub async fn create_workflow_def(&self, def: WorkflowDef) -> anyhow::Result<()> {
        let def = validate_and_normalize_workflow_def(def)?;
        self.storage.save_workflow_def(def).await
    }

    pub async fn create_workflow_instance_for_def(
        &self,
        workflow_def_id: &str,
    ) -> anyhow::Result<String> {
        let Some(_) = self.storage.get_workflow_def(workflow_def_id).await? else {
            anyhow::bail!("workflow definition {workflow_def_id} not found");
        };

        let instance_id = create_instance_id(workflow_def_id)?;
        let instance = WorkflowInstance {
            id: instance_id.clone(),
            workflow_def_id: workflow_def_id.to_string(),
            status: WorkflowStatus::Pending,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        };

        self.storage.save_workflow_instance(instance).await?;

        Ok(instance_id)
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
        requested_task_id: &str,
    ) -> anyhow::Result<TaskResult> {
        self.get_task_result_for_generation(workflow_instance_id, requested_task_id, None)
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
            .map(|(task_attempt_id, task)| WorkflowTaskResult {
                task_attempt_id: task_attempt_id.clone(),
                result: task_result_for_instance(
                    task_attempt_id,
                    task,
                    should_include_task_result_metadata(
                        task_attempt_id,
                        task_attempt_id,
                        task,
                        None,
                    ),
                ),
            })
            .collect();
        tasks.sort_by(|left, right| left.task_attempt_id.cmp(&right.task_attempt_id));

        Ok(tasks)
    }

    pub async fn get_task_result_for_generation(
        &self,
        workflow_instance_id: &str,
        requested_task_id: &str,
        generation: Option<u32>,
    ) -> anyhow::Result<TaskResult> {
        let instance = self
            .storage
            .get_workflow_instance(workflow_instance_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("workflow instance {workflow_instance_id} not found"))?;

        let task_attempt_id = resolve_task_attempt_id(&instance, requested_task_id, generation)?;

        let task = instance
            .tasks
            .get(&task_attempt_id)
            .ok_or_else(|| anyhow::anyhow!("task {requested_task_id} not found"))?;

        Ok(task_result_for_instance(
            &task_attempt_id,
            task,
            should_include_task_result_metadata(
                requested_task_id,
                &task_attempt_id,
                task,
                generation,
            ),
        ))
    }
}

fn resolve_task_attempt_id(
    instance: &WorkflowInstance,
    requested_task_id: &str,
    requested_generation: Option<u32>,
) -> anyhow::Result<String> {
    let normalized_task_id = requested_task_id.to_ascii_lowercase();

    if let Some(generation) = requested_generation {
        if generation == 0 {
            anyhow::bail!("generation must be positive");
        }
        return Ok(TaskInstance::make_task_attempt_id(
            &normalized_task_id,
            generation,
        ));
    }

    if instance.tasks.contains_key(requested_task_id) {
        return Ok(requested_task_id.to_string());
    }

    if normalized_task_id != requested_task_id && instance.tasks.contains_key(&normalized_task_id) {
        return Ok(normalized_task_id);
    }

    if let Some((task_attempt_id, _)) = instance
        .tasks
        .iter()
        .filter(|(_, task)| task.task_def_id == normalized_task_id)
        .max_by_key(|(_, task)| task.generation_index)
    {
        return Ok(task_attempt_id.clone());
    }

    anyhow::bail!("task {requested_task_id} not found")
}

fn should_include_task_result_metadata(
    requested_task_id: &str,
    task_attempt_id: &str,
    task: &TaskInstance,
    requested_generation: Option<u32>,
) -> bool {
    requested_generation.is_some()
        || requested_task_id != task_attempt_id
        || task.generation_index > 0
        || task.verifier_metadata.is_some()
}

fn task_result_for_instance(
    task_attempt_id: &str,
    task: &TaskInstance,
    include_metadata: bool,
) -> TaskResult {
    let metadata = include_metadata.then(|| TaskResultMetadata {
        task_def_id: task.task_def_id.clone(),
        task_attempt_id: task_attempt_id.to_string(),
        satisfaction: task.satisfaction_status.clone(),
        input_mapping: task.input_mapping.clone(),
        generation_index: task.generation_index,
        verifier_metadata: task.verifier_metadata.clone(),
    });

    match &task.status {
        TaskStatus::Completed => TaskResult::Success {
            input: task.input_data.clone(),
            output: task.output_data.clone().unwrap_or(serde_json::Value::Null),
            metadata,
        },
        TaskStatus::Failed => TaskResult::Failure {
            input: task.input_data.clone(),
            error_message: "task failed".to_string(),
            metadata,
        },
        TaskStatus::Pending => TaskResult::Pending {
            input: task.input_data.clone(),
            metadata,
        },
        TaskStatus::Running | TaskStatus::InputNeeded { .. } => TaskResult::Running {
            input: task.input_data.clone(),
            metadata,
        },
    }
}

fn create_instance_id(workflow_def_id: &str) -> anyhow::Result<String> {
    let timestamp_nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(format!("{workflow_def_id}-{timestamp_nanos}"))
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
    validate_identifier("workflow", &def.id)?;
    def.id = def.id.to_ascii_lowercase();

    let mut original_to_normalized = HashMap::new();
    let mut normalized_task_ids = HashSet::new();

    for task in &mut def.tasks {
        validate_identifier("task", &task.id)?;
        let normalized = task.id.to_ascii_lowercase();
        if !normalized_task_ids.insert(normalized.clone()) {
            anyhow::bail!("duplicate task id after normalization: {normalized}");
        }
        original_to_normalized.insert(task.id.clone(), normalized.clone());
        task.id = normalized;
        if let Some(workspace) = task.workspace.as_mut() {
            validate_identifier("workspace group", &workspace.group_name)?;
            workspace.group_name = workspace.group_name.to_ascii_lowercase();
        }
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

    validate_non_overlapping_verifier_slices(&def)?;

    Ok(def)
}

fn validate_identifier(kind: &str, id: &str) -> anyhow::Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        anyhow::bail!(
            "{kind} id {id:?} must contain only ASCII alphanumeric characters, '-' or '_'"
        );
    }
    Ok(())
}

fn normalize_task_reference(
    original_to_normalized: &HashMap<String, String>,
    id: &str,
    field: &str,
) -> anyhow::Result<String> {
    validate_identifier(field, id)?;
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

fn validate_non_overlapping_verifier_slices(def: &WorkflowDef) -> anyhow::Result<()> {
    let loop_slices = compute_loop_slices(def);
    let mut owners: HashMap<String, String> = HashMap::new();

    for (verifier_id, slice) in loop_slices {
        for task_id in slice {
            if let Some(existing_verifier_id) = owners.insert(task_id.clone(), verifier_id.clone())
            {
                anyhow::bail!(
                    "verifier loop slices overlap on task {task_id}: {existing_verifier_id} and {verifier_id}"
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::memory_storage::MemoryStorage;
    use crate::core::models::{FunctionTaskDef, TaskTypeDef};
    use serde_json::json;

    fn workflow_def(id: &str) -> WorkflowDef {
        WorkflowDef {
            id: id.to_string(),
            tasks: vec![TaskDef {
                id: "taska".to_string(),
                kind: TaskTypeDef::Function(FunctionTaskDef::Inline {
                    dependencies: vec![],
                    code: "export default async function run() { return {}; }".to_string(),
                }),
                control: None,
                timeout_secs: None,
                input_schemas: vec![],
                output_schema: Some(json!({ "type": "object" })),
                workspace: None,
                required_credentials: vec![],
            }],
            data_bindings: vec![],
        }
    }

    #[tokio::test]
    async fn create_workflow_instance_for_def_persists_pending_instance() {
        let storage = Arc::new(MemoryStorage::new());
        let service = WorkflowService::new(storage.clone());
        service
            .create_workflow_def(workflow_def("workflow1"))
            .await
            .unwrap();

        let instance_id = service
            .create_workflow_instance_for_def("workflow1")
            .await
            .unwrap();

        let instance = storage
            .get_workflow_instance(&instance_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(instance.workflow_def_id, "workflow1");
        assert_eq!(instance.status, WorkflowStatus::Pending);
        assert!(instance.tasks.is_empty());
        assert!(instance.verifier_states.is_empty());
    }

    #[tokio::test]
    async fn create_workflow_instance_for_def_rejects_unknown_definition() {
        let service = WorkflowService::new(Arc::new(MemoryStorage::new()));

        let error = service
            .create_workflow_instance_for_def("missing")
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("workflow definition missing not found")
        );
    }

    #[tokio::test]
    async fn list_workflows_filters_by_status() {
        let storage = Arc::new(MemoryStorage::new());
        let service = WorkflowService::new(storage.clone());
        let completed = WorkflowInstance {
            id: "completed-workflow".to_string(),
            workflow_def_id: "workflow-1".to_string(),
            status: WorkflowStatus::Completed,
            tasks: HashMap::new(),
            verifier_states: HashMap::new(),
        };
        let mut running = completed.clone();
        running.id = "running-workflow".to_string();
        running.status = WorkflowStatus::Running;

        storage.save_workflow_instance(completed).await.unwrap();
        storage.save_workflow_instance(running).await.unwrap();

        let workflows = service
            .list_workflows(Some(WorkflowStatus::Running))
            .await
            .unwrap();

        assert_eq!(workflows.workflows.len(), 1);
        assert_eq!(workflows.workflows[0].id, "running-workflow");
        assert_eq!(workflows.workflows[0].workflow_def_id, "workflow-1");
        assert_eq!(workflows.workflows[0].status, WorkflowStatus::Running);
    }
}
