use crate::core::function_resolution::resolve_task_function_ref;
use crate::core::models::{
    TaskStatus, TaskStatusReport, WorkflowDef, WorkflowInstance, WorkflowStatus,
    WorkflowStatusReport,
};
use crate::ports::executor::{ExecutionResult, ExecutorPort};
use crate::ports::storage::StoragePort;
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
                status: t.status.clone(),
                has_output: t.output_data.is_some(),
            })
            .collect();

        // Sort for deterministic ordering.
        tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));

        Ok(Some(WorkflowStatusReport {
            instance_id: instance.id,
            workflow_def_id: instance.workflow_def_id,
            status: instance.status,
            tasks,
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

        // Initialize tasks if not already done
        if instance.tasks.is_empty() {
            for task_def in &def.tasks {
                instance.tasks.insert(
                    task_def.id.clone(),
                    crate::core::models::TaskInstance {
                        task_def_id: task_def.id.clone(),
                        status: TaskStatus::Pending,
                        input_data: vec![], // Empty until upstream dependencies propagate data
                        output_data: None,
                        recorded_side_effects: vec![],
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

            let mut tasks_to_run = Vec::new();

            for (task_id, task_instance) in instance.tasks.iter() {
                if task_instance.status == TaskStatus::Pending {
                    let task_def = def.tasks.iter().find(|t| t.id == *task_id).unwrap();
                    let can_run = self.are_inputs_satisfied(&instance, &def, task_def);
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

                let task_def = def.tasks.iter().find(|t| t.id == task_id).unwrap();

                // Collect already-propagated inputs for this task (empty slice if none yet).
                let inputs: Vec<serde_json::Value> = instance
                    .tasks
                    .get(&task_id)
                    .map(|t| t.input_data.clone())
                    .unwrap_or_default();

                let execution_result =
                    match resolve_task_function_ref(self.storage.as_ref(), task_def).await {
                        Ok(resolved_task_def) => {
                            self.executor.execute(&resolved_task_def, &inputs).await
                        }
                        Err(error) => Err(error),
                    };

                match execution_result {
                    Ok(ExecutionResult::Success(output)) => {
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
                            self.propagate_data(&mut instance, &def, &task_id, &output);
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
                    Ok(ExecutionResult::InputNeeded(description)) => {
                        if let Some(task) = instance.tasks.get_mut(&task_id) {
                            task.status = TaskStatus::InputNeeded { description };
                        }
                        instance.status = WorkflowStatus::InputNeeded;
                    }
                    Ok(ExecutionResult::Failure(reason)) => {
                        if let Some(task) = instance.tasks.get_mut(&task_id) {
                            task.status = TaskStatus::Failed;
                        }
                        instance.status = WorkflowStatus::Failed;
                        self.storage
                            .save_workflow_instance(instance.clone())
                            .await?;
                        anyhow::bail!("Task execution failed: {}", reason);
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
            .all(|t| t.status == TaskStatus::Completed);
        if all_completed {
            instance.status = WorkflowStatus::Completed;
            self.storage.save_workflow_instance(instance).await?;
        }

        Ok(())
    }

    fn are_inputs_satisfied(
        &self,
        instance: &WorkflowInstance,
        def: &WorkflowDef,
        task_def: &crate::core::models::TaskDef,
    ) -> bool {
        // A task's inputs are satisfied if all of its upstream dependencies have Completed
        for binding in &def.data_bindings {
            if binding.target_task_id == task_def.id {
                if let Some(source_task) = instance.tasks.get(&binding.source_task_id) {
                    if source_task.status != TaskStatus::Completed {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn propagate_data(
        &self,
        instance: &mut WorkflowInstance,
        def: &WorkflowDef,
        source_id: &str,
        output: &serde_json::Value,
    ) {
        // Propagate output data to the inputs of downstream tasks
        for binding in &def.data_bindings {
            if binding.source_task_id == source_id {
                let target_id = &binding.target_task_id;
                if let Some(target_task) = instance.tasks.get_mut(target_id) {
                    // Expand vec to fit the target index
                    while target_task.input_data.len() <= binding.target_input_index {
                        target_task.input_data.push(serde_json::Value::Null);
                    }
                    target_task.input_data[binding.target_input_index] = output.clone();
                }
            }
        }
    }

    fn detect_cycles(&self, def: &WorkflowDef) -> bool {
        use std::collections::HashMap;

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for task in &def.tasks {
            in_degree.insert(task.id.clone(), 0);
            adj.insert(task.id.clone(), Vec::new());
        }

        for binding in &def.data_bindings {
            *in_degree.entry(binding.target_task_id.clone()).or_insert(0) += 1;
            adj.entry(binding.source_task_id.clone())
                .or_insert_with(Vec::new)
                .push(binding.target_task_id.clone());
        }

        let mut queue: Vec<String> = Vec::new();
        for (node, degree) in &in_degree {
            if *degree == 0 {
                queue.push(node.clone());
            }
        }

        let mut visited_count = 0;
        while let Some(node) = queue.pop() {
            visited_count += 1;
            if let Some(neighbors) = adj.get(&node) {
                for neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(neighbor.clone());
                        }
                    }
                }
            }
        }

        visited_count != def.tasks.len()
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
