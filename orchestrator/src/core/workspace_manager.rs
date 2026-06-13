use std::{path::{Path, PathBuf}, time::Duration};
use anyhow::{Result};
use serde::{Deserialize, Serialize};

use crate::core::models::TaskDef;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkspaceKey {
    Task {
        workflow_inst_id: String,
        task_id: String,
    },
    Group {
        workflow_inst_id: String,
        group_name: String,
    },
}

pub struct WorkspaceManager {
    config: WorkspaceManagerConfig
}

pub struct WorkspaceManagerConfig {
    pub root: PathBuf,
    pub ttl: Duration,
    pub vacuum_interval: Duration
}

impl WorkspaceManager {
    pub fn new(config: WorkspaceManagerConfig) -> WorkspaceManager {
        todo!();
    }

    pub fn get_path(&self, key: &WorkspaceKey) -> Result<PathBuf> {
        todo!();
    }

    pub async fn vacuum() {
        todo!();
    }
}

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
        WorkspaceKey::Task {
            workflow_inst_id,
            task_id,
        } => root
            .join(workflow_inst_id)
            .join(format!("taskid-{}", task_id))
            .to_path_buf(),
        WorkspaceKey::Group {
            workflow_inst_id,
            group_name,
        } => root
            .join(workflow_inst_id)
            .join(format!("taskgroup-{}", group_name))
            .to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use crate::core::{
        models::{FunctionTaskDef, TaskDef, TaskTypeDef, Workspace}, workspace_manager::{WorkspaceKey, workspace_key_for_task, workspace_path}
    };
    use serde_json::json;
    use std::path::Path;

    fn task(id: &str, group_name: Option<&str>) -> TaskDef {
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
            workspace: group_name.map(|group_name| Workspace {
                group_name: group_name.to_string(),
            }),
            required_credentials: vec![],
        }
    }

    #[test]
    fn default_private_workspace_selection_uses_task_key() {
        let task = task("foo", None);
        let key = workspace_key_for_task("123", &task);

        assert_eq!(
            key,
            WorkspaceKey::Task {
                workflow_inst_id: "123".to_string(),
                task_id: "foo".to_string(),
            }
        );
    }

    #[test]
    fn workspace_group_override_selection_uses_group_key() {
        let task = task("bar", Some("foobar"));
        let key = workspace_key_for_task("123", &task);

        assert_eq!(
            key,
            WorkspaceKey::Group {
                workflow_inst_id: "123".to_string(),
                group_name: "foobar".to_string(),
            }
        );
    }

    #[test]
    fn same_workspace_group_resolves_to_same_key_across_tasks() {
        let first_task = task("clone", Some("repo"));
        let second_task = task("inspect", Some("repo"));

        assert_eq!(
            workspace_key_for_task("workflow1", &first_task),
            workspace_key_for_task("workflow1", &second_task)
        );
    }

    #[test]
    fn workspace_path_for_private_task_uses_task_namespace() {
        let expected = "/root/workspace/1234/taskid-123";

        let workspace_key = WorkspaceKey::Task {
            workflow_inst_id: "1234".to_string(),
            task_id: "123".to_owned(),
        };
        let given = workspace_path(Path::new("/root/workspace"), &workspace_key);

        assert_eq!(expected.to_string(), given.to_str().to_owned().unwrap());
    }

    #[test]
    fn workspace_path_for_group_uses_group_namespace() {
        let expected = "/root/workspace/1234/taskgroup-foobar";

        let workspace_key = WorkspaceKey::Group {
            workflow_inst_id: "1234".to_string(),
            group_name: "foobar".to_string(),
        };
        let given = workspace_path(Path::new("/root/workspace"), &workspace_key);

        assert_eq!(expected.to_string(), given.to_str().to_owned().unwrap());
    }

    #[test]
    fn task_and_group_with_same_name_do_not_share_workspace_path() {
        let private_task_key = WorkspaceKey::Task {
            workflow_inst_id: "workflow1".to_string(),
            task_id: "repo".to_string(),
        };
        let group_key = WorkspaceKey::Group {
            workflow_inst_id: "workflow1".to_string(),
            group_name: "repo".to_string(),
        };

        assert_ne!(
            workspace_path(Path::new("/root/workspace"), &private_task_key),
            workspace_path(Path::new("/root/workspace"), &group_key)
        );
    }
}