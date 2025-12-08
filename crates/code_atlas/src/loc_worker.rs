use anyhow::Result;
use gpui::{BackgroundExecutor, Task};
use project::ProjectEntryId;
use std::io::{BufReader, Read};
use std::path::PathBuf;

use crate::persistence::CODE_ATLAS_DB;

#[derive(Clone, Debug)]
pub struct LocResult {
    pub entry_id: ProjectEntryId,
    pub loc: u64,
}

pub fn count_lines(path: &std::path::Path) -> Result<u64> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut count = 0u64;
    let mut buf = [0u8; 32768];
    loop {
        let bytes_read = reader.read(&mut buf)?;
        if bytes_read == 0 {
            break;
        }
        count += bytecount::count(&buf[..bytes_read], b'\n') as u64;
    }
    Ok(count)
}

pub fn spawn_loc_worker(
    executor: BackgroundExecutor,
    files: Vec<(ProjectEntryId, PathBuf, i64, i64, i32)>,
    on_batch_complete: impl FnOnce(Vec<LocResult>) + Send + 'static,
) -> Task<()> {
    executor.spawn(async move {
        let mut results = Vec::with_capacity(files.len());

        for (entry_id, path, worktree_id, mtime_seconds, mtime_nanos) in files {
            if let Ok(Some(cached_loc)) =
                CODE_ATLAS_DB.get_loc(worktree_id, entry_id.to_proto() as i64, mtime_seconds, mtime_nanos)
            {
                results.push(LocResult {
                    entry_id,
                    loc: cached_loc as u64,
                });
                continue;
            }

            match count_lines(&path) {
                Ok(loc) => {
                    let _ = CODE_ATLAS_DB
                        .save_loc(
                            worktree_id,
                            entry_id.to_proto() as i64,
                            path.to_string_lossy().to_string(),
                            loc as i64,
                            mtime_seconds,
                            mtime_nanos,
                        )
                        .await;

                    results.push(LocResult { entry_id, loc });
                }
                Err(e) => {
                    log::debug!("Failed to count lines for {:?}: {}", path, e);
                }
            }
        }

        on_batch_complete(results);
    })
}
