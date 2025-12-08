use anyhow::Result;
use gpui::{BackgroundExecutor, Task};
use project::ProjectEntryId;
use smol::process::Command;
use std::path::PathBuf;

use crate::persistence::CODE_ATLAS_DB;

#[derive(Clone, Debug)]
pub struct GitResult {
    pub entry_id: ProjectEntryId,
    pub timestamp: Option<i64>,
    #[allow(dead_code)]
    pub author: Option<String>,
    #[allow(dead_code)]
    pub worktree_id: i64,
    #[allow(dead_code)]
    pub path: String,
    #[allow(dead_code)]
    pub mtime_s: i64,
    #[allow(dead_code)]
    pub mtime_ns: i32,
}

/// Gets last commit info for a file using git log (async version)
async fn get_last_commit_info(
    repo_path: &std::path::Path,
    file_path: &std::path::Path,
) -> Result<(Option<i64>, Option<String>)> {
    // git log -1 --format=%at%n%an -- <file>
    // %at: author time, unix timestamp
    // %an: author name
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["log", "-1", "--format=%at\n%an", "--"])
        .arg(file_path)
        .output()
        .await?;

    if !output.status.success() {
        return Ok((None, None));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    let timestamp = lines.next().and_then(|s| s.parse::<i64>().ok());
    let author = lines.next().map(|s| s.to_string());

    Ok((timestamp, author))
}

pub fn spawn_git_worker(
    executor: BackgroundExecutor,
    repo_path: PathBuf,
    files: Vec<(ProjectEntryId, PathBuf, i64, i64, i32)>, // entry_id, relative_path, worktree_id, mtime_s, mtime_ns
    mut on_result: impl FnMut(GitResult) + Send + 'static,
) -> Task<()> {
    executor.spawn(async move {
        for (entry_id, file_path, worktree_id, mtime_s, mtime_ns) in files {
            // Check cache first (includes mtime validation for cache invalidation)
            if let Ok(Some((timestamp, author))) = CODE_ATLAS_DB
                .get_git_info(worktree_id, entry_id.to_proto() as i64, mtime_s, mtime_ns)
            {
                on_result(GitResult {
                    entry_id,
                    timestamp,
                    author,
                    worktree_id,
                    path: file_path.to_string_lossy().to_string(),
                    mtime_s,
                    mtime_ns,
                });
                continue;
            }

            let relative_path = file_path.strip_prefix(&repo_path).unwrap_or(&file_path);

            match get_last_commit_info(&repo_path, relative_path).await {
                Ok((timestamp, author)) => {
                    // Cache result
                    let _ = CODE_ATLAS_DB
                        .save_git_info(
                            worktree_id,
                            entry_id.to_proto() as i64,
                            file_path.to_string_lossy().to_string(),
                            timestamp,
                            author.clone(),
                            mtime_s,
                            mtime_ns,
                        )
                        .await;

                    on_result(GitResult {
                        entry_id,
                        timestamp,
                        author,
                        worktree_id,
                        path: file_path.to_string_lossy().to_string(),
                        mtime_s,
                        mtime_ns,
                    });
                }
                Err(_) => {
                    on_result(GitResult {
                        entry_id,
                        timestamp: None,
                        author: None,
                        worktree_id,
                        path: file_path.to_string_lossy().to_string(),
                        mtime_s,
                        mtime_ns,
                    });
                }
            }
        }
    })
}
