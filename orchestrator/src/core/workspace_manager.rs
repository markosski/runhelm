use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::core::{models::TaskDef, util::unix_timestamp};

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
    config: WorkspaceManagerConfig,
}

pub struct WorkspaceManagerConfig {
    pub root: PathBuf,
    pub ttl: Duration,
    pub vacuum_interval: Duration,
}

impl WorkspaceManager {
    pub fn new(config: WorkspaceManagerConfig) -> WorkspaceManager {
        WorkspaceManager { config }
    }

    pub fn default() -> WorkspaceManager {
        let root = get_workspace_root();

        WorkspaceManager::new(WorkspaceManagerConfig {
            root: PathBuf::from(root),
            ttl: Duration::from_secs(3600),
            vacuum_interval: Duration::from_secs(60),
        })
    }

    pub fn ensure_workspace(&self, workflow_inst_id: &str, task: &TaskDef) -> Result<PathBuf> {
        let key = workspace_key_for_task(workflow_inst_id, task);
        let full_path = workspace_path(&self.config.root, &key);
        if full_path.is_dir() {
            Ok(full_path)
        } else {
            fs::create_dir_all(&full_path)?;
            Ok(full_path)
        }
    }

    /// Updates the timestamp file in workspace directory.
    pub fn create_or_time_stamp_workspace(
        &self,
        workflow_inst_id: &str,
        task: &TaskDef,
    ) -> Result<PathBuf> {
        let workspace_path = self.ensure_workspace(workflow_inst_id, task)?;
        let file_path = workspace_path.join(".timestamp");
        fs::write(file_path, unix_timestamp()?.to_string())?;

        Ok(workspace_path)
    }

    pub async fn vacuum(&self) -> Result<()> {
        if !self.config.root.is_dir() {
            return Ok(());
        }

        let now = unix_timestamp()?;
        let expiration_cutoff = now.saturating_sub(self.config.ttl.as_secs());

        for workflow_entry in fs::read_dir(&self.config.root)? {
            let workflow_entry = workflow_entry?;
            let workflow_path = workflow_entry.path();

            if !workflow_path.is_dir() {
                continue;
            }

            for workspace_entry in fs::read_dir(&workflow_path)? {
                let workspace_entry = workspace_entry?;
                let workspace_path = workspace_entry.path();

                if !workspace_path.is_dir() {
                    continue;
                }

                let timestamp_path = workspace_path.join(".timestamp");
                let timestamp = match fs::read_to_string(timestamp_path) {
                    Ok(contents) => contents.trim().parse::<u64>().ok(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.into()),
                };

                if timestamp.is_some_and(|timestamp| timestamp <= expiration_cutoff) {
                    fs::remove_dir_all(&workspace_path)?;
                }
            }

            if fs::read_dir(&workflow_path)?.next().is_none() {
                fs::remove_dir(&workflow_path)?;
            }
        }

        Ok(())
    }
}

fn get_workspace_root() -> PathBuf {
    std::env::var("RUNHELM_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|home| {
                    PathBuf::from(home)
                        .join(".cache")
                        .join("runhelm")
                        .join("workspaces")
                })
                .unwrap()
        })
}

fn workspace_key_for_task(workflow_inst_id: &str, task: &TaskDef) -> WorkspaceKey {
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
        models::{FunctionTaskDef, TaskDef, TaskTypeDef, Workspace},
        workspace_manager::{
            WorkspaceKey, WorkspaceManager, WorkspaceManagerConfig, workspace_key_for_task,
            workspace_path,
        },
    };
    use serde_json::json;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn temp_root(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!(
            "runhelm-{}-{}-{}",
            test_name,
            std::process::id(),
            nanos
        ))
    }

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

    #[test]
    fn ensure_workspace_called_multiple_times() {
        let root = temp_root("task-workspace-reuse");

        let config = WorkspaceManagerConfig {
            root: root.clone(),
            ttl: Duration::from_secs(10),
            vacuum_interval: Duration::from_secs(10),
        };
        let workspace_manager = WorkspaceManager::new(config);

        let task_def = task("test", None);
        let new_path = workspace_manager.ensure_workspace("workflow1", &task_def);

        assert!(new_path.is_ok());

        let new_path_2 = workspace_manager.ensure_workspace("workflow1", &task_def);
        assert!(new_path_2.is_ok());
        assert_eq!(new_path.unwrap(), new_path_2.unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_workspace_called_multiple_times_with_group() {
        let root = temp_root("group-workspace-reuse");

        let config = WorkspaceManagerConfig {
            root: root.clone(),
            ttl: Duration::from_secs(10),
            vacuum_interval: Duration::from_secs(10),
        };
        let workspace_manager = WorkspaceManager::new(config);

        let task_def = task("test", Some("foobar"));
        let new_path = workspace_manager.ensure_workspace("workflow1", &task_def);

        assert!(new_path.is_ok());

        let new_path_2 = workspace_manager.ensure_workspace("workflow1", &task_def);
        assert!(new_path_2.is_ok());
        assert_eq!(new_path.unwrap(), new_path_2.unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_workspace_with_shared_group_among_tasks() {
        let root = temp_root("shared-group-workspace");

        let config = WorkspaceManagerConfig {
            root: root.clone(),
            ttl: Duration::from_secs(10),
            vacuum_interval: Duration::from_secs(10),
        };
        let workspace_manager = WorkspaceManager::new(config);

        let task_def_1 = task("test-1", Some("foobar"));
        let workspace_path_1 = workspace_manager.ensure_workspace("workflow1", &task_def_1);

        let task_def_2 = task("test-2", Some("foobar"));
        let workspace_path_2 = workspace_manager.ensure_workspace("workflow1", &task_def_2);

        assert_eq!(workspace_path_1.unwrap(), workspace_path_2.unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn vacuum_deletes_expired_workspaces_and_keeps_fresh_workspaces() {
        let root = temp_root("vacuum");
        let workflow_path = root.join("workflow1");
        let expired_workspace = workflow_path.join("taskid-expired");
        let fresh_workspace = workflow_path.join("taskid-fresh");
        fs::create_dir_all(&expired_workspace).unwrap();
        fs::create_dir_all(&fresh_workspace).unwrap();
        fs::write(expired_workspace.join(".timestamp"), "100").unwrap();
        fs::write(fresh_workspace.join(".timestamp"), u64::MAX.to_string()).unwrap();

        let manager = WorkspaceManager::new(WorkspaceManagerConfig {
            root: root.clone(),
            ttl: Duration::from_secs(60),
            vacuum_interval: Duration::from_secs(60),
        });

        manager.vacuum().await.unwrap();

        assert!(!expired_workspace.exists());
        assert!(fresh_workspace.exists());

        fs::remove_dir_all(root).unwrap();
    }
}
