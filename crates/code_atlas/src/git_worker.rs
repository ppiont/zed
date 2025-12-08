use gpui::{BackgroundExecutor, Task};
use project::ProjectEntryId;
use smol::process::Command;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::persistence::CODE_ATLAS_DB;

#[derive(Clone, Debug)]
pub struct GitResult {
    pub entry_id: ProjectEntryId,
    pub timestamp: Option<i64>,
}

pub fn spawn_git_worker(
    executor: BackgroundExecutor,
    repo_path: PathBuf,
    files: Vec<(ProjectEntryId, PathBuf, i64, i64, i32)>,
    mut on_result: impl FnMut(GitResult) + Send + 'static,
) -> Task<()> {
    executor.spawn(async move {
        // Build a map from relative path to file info for quick lookup
        let mut path_to_info: HashMap<String, (ProjectEntryId, i64, i64, i32)> = HashMap::new();
        let mut uncached_paths: Vec<String> = Vec::new();

        for (entry_id, file_path, worktree_id, mtime_s, mtime_ns) in &files {
            let rel_path = file_path
                .strip_prefix(&repo_path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            // Check cache first
            if let Ok(Some(timestamp)) =
                CODE_ATLAS_DB.get_git_timestamp(*worktree_id, entry_id.to_proto() as i64, *mtime_s, *mtime_ns)
            {
                on_result(GitResult {
                    entry_id: *entry_id,
                    timestamp: Some(timestamp),
                });
                continue;
            }

            path_to_info.insert(rel_path.clone(), (*entry_id, *worktree_id, *mtime_s, *mtime_ns));
            uncached_paths.push(rel_path);
        }

        if uncached_paths.is_empty() {
            return;
        }

        // Get all file timestamps in one git command
        // Format: timestamp\nfile1\nfile2\n\ntimestamp\nfile3\n...
        // (empty line separates commits)
        let output = Command::new("git")
            .current_dir(&repo_path)
            .args(["log", "--format=%at", "--name-only", "--diff-filter=ACMR"])
            .output()
            .await;

        let Ok(output) = output else {
            for rel_path in &uncached_paths {
                if let Some((entry_id, _, _, _)) = path_to_info.get(rel_path) {
                    on_result(GitResult {
                        entry_id: *entry_id,
                        timestamp: None,
                    });
                }
            }
            return;
        };

        if !output.status.success() {
            for rel_path in &uncached_paths {
                if let Some((entry_id, _, _, _)) = path_to_info.get(rel_path) {
                    on_result(GitResult {
                        entry_id: *entry_id,
                        timestamp: None,
                    });
                }
            }
            return;
        }

        // Parse output format:
        // timestamp
        // <empty line>
        // file1
        // file2
        // timestamp
        // <empty line>
        // file3
        // ...
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut file_timestamps: HashMap<String, i64> = HashMap::new();

        let mut current_timestamp: Option<i64> = None;
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                // Empty line after timestamp - just skip it
                continue;
            }

            // Try to parse as timestamp first
            if let Ok(ts) = line.parse::<i64>() {
                current_timestamp = Some(ts);
            } else if let Some(ts) = current_timestamp {
                // This is a file path - only record first occurrence (most recent)
                file_timestamps.entry(line.to_string()).or_insert(ts);
            }
        }

        // Report results and cache them
        for rel_path in &uncached_paths {
            if let Some((entry_id, worktree_id, mtime_s, mtime_ns)) = path_to_info.get(rel_path) {
                let timestamp = file_timestamps.get(rel_path).copied();

                // Cache the result
                if let Some(ts) = timestamp {
                    let _ = CODE_ATLAS_DB
                        .save_git_timestamp(*worktree_id, entry_id.to_proto() as i64, ts, *mtime_s, *mtime_ns)
                        .await;
                }

                on_result(GitResult {
                    entry_id: *entry_id,
                    timestamp,
                });
            }
        }
    })
}
