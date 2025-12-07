use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};

pub struct CodeAtlasDb(ThreadSafeConnection);

impl Domain for CodeAtlasDb {
    const NAME: &str = stringify!(CodeAtlasDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE IF NOT EXISTS loc_cache(
            worktree_id INTEGER NOT NULL,
            entry_id INTEGER NOT NULL,
            path TEXT NOT NULL,
            loc INTEGER NOT NULL,
            mtime_seconds INTEGER NOT NULL,
            mtime_nanos INTEGER NOT NULL,
            PRIMARY KEY(worktree_id, entry_id)
        ) STRICT;
    )];
}

db::static_connection!(CODE_ATLAS_DB, CodeAtlasDb, []);

impl CodeAtlasDb {
    query! {
        pub fn get_loc(worktree_id: i64, entry_id: i64, mtime_seconds: i64, mtime_nanos: i32) -> Result<Option<i64>> {
            SELECT loc FROM loc_cache
            WHERE worktree_id = ?1
              AND entry_id = ?2
              AND mtime_seconds = ?3
              AND mtime_nanos = ?4
        }
    }

    query! {
        pub async fn save_loc(worktree_id: i64, entry_id: i64, path: String, loc: i64, mtime_seconds: i64, mtime_nanos: i32) -> Result<()> {
            INSERT OR REPLACE INTO loc_cache(
                worktree_id, entry_id, path, loc, mtime_seconds, mtime_nanos
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        }
    }
}
