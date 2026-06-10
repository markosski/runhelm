use std::path::{Path, PathBuf};

use crate::core::{models::TaskDef, workflow::models::WorkspaceKey};

pub mod workflow_service;
pub mod models;

pub fn workspace_key_for_task(workflow_inst_id: &str, task: &TaskDef) -> WorkspaceKey {
    match &task.workspace {
        Some(workspace) => WorkspaceKey::Group {
            workflow_inst_id: workflow_inst_id.to_string(),
            group_name: workspace.group_name.clone(),
        },
        None => WorkspaceKey::Task {
            workflow_inst_id: workflow_inst_id.to_string(),
            task_id: task.id.clone(),
        },
    }
}

pub fn workspace_path(root: &Path, key: &WorkspaceKey) -> PathBuf {
    match key {
        WorkspaceKey::Task { workflow_inst_id, task_id } => {
            root.join(workflow_inst_id).join(format!("taskid-{}", task_id)).to_path_buf()
        },
        WorkspaceKey::Group {workflow_inst_id, group_name } => {
            root.join(workflow_inst_id).join(format!("taskgroup-{}", group_name)).to_path_buf()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use serde_json::json;
    use crate::core::{
        models::{FunctionTaskDef, TaskDef, TaskTypeDef, Workspace}, 
        workflow::{models::WorkspaceKey, workspace_key_for_task, workspace_path}
    };

    fn task(id: &str, is_group: bool) -> TaskDef {
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
            workspace: if is_group {
               Some(Workspace { group_name: "foobar".to_string() }) 
            } else {
                None
            },
            required_credentials: vec![],
        }
    }

    #[test]
    fn test_create_workspace_key_for_task() {
        let task = task("foo", false);
        let key = workspace_key_for_task("123", &task);

        assert!(matches!(key, WorkspaceKey::Task { workflow_inst_id, task_id } if workflow_inst_id == "123" && task_id == "foo"));
    }

    #[test]
    fn test_create_workspace_key_for_task_group() {
        let task = task("bar", true);
        let key = workspace_key_for_task("123", &task);

        assert!(matches!(key, WorkspaceKey::Group { workflow_inst_id, group_name } if workflow_inst_id == "123" && group_name == "foobar"));
    }

    #[test]
    fn test_workspace_path_task() {
        let expected = "/root/workspace/1234/taskid-123";

        let workspace_key = WorkspaceKey::Task { workflow_inst_id: "1234".to_string(), task_id: "123".to_owned() };
        let given = workspace_path(Path::new("/root/workspace"), &workspace_key);

        assert_eq!(expected.to_string(), given.to_str().to_owned().unwrap());
    }

    #[test]
    fn test_workspace_path_group() {
        let expected = "/root/workspace/1234/taskgroup-foobar";

        let workspace_key = WorkspaceKey::Group { workflow_inst_id: "1234".to_string(), group_name: "foobar".to_string() };
        let given = workspace_path(Path::new("/root/workspace"), &workspace_key);

        assert_eq!(expected.to_string(), given.to_str().to_owned().unwrap());
    }
}