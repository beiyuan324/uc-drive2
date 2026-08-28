//! SQLite 打开与建表。
//! 文件系统为事实源，files 表只存派生缓存；tasks 表为任务记录。

use rusqlite::Connection;

pub fn open_db(file: &std::path::Path) -> Connection {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = Connection::open(file).expect("无法打开 SQLite 数据库");
    db.pragma_update(None, "journal_mode", "WAL").ok();
    db.pragma_update(None, "foreign_keys", "ON").ok();
    db.execute_batch(
        r#"
    CREATE TABLE IF NOT EXISTS files (
      id         INTEGER PRIMARY KEY AUTOINCREMENT,
      name       TEXT NOT NULL,
      parent_id  INTEGER REFERENCES files(id) ON DELETE CASCADE,
      is_dir     INTEGER NOT NULL DEFAULT 0,
      path       TEXT NOT NULL UNIQUE,
      size       INTEGER NOT NULL DEFAULT 0,
      mime       TEXT NOT NULL DEFAULT '',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_files_parent ON files(parent_id);

    CREATE TABLE IF NOT EXISTS tasks (
      id          INTEGER PRIMARY KEY AUTOINCREMENT,
      gopeed_id   TEXT NOT NULL DEFAULT '',
      source      TEXT NOT NULL DEFAULT 'url',
      source_url  TEXT NOT NULL DEFAULT '',
      status      TEXT NOT NULL DEFAULT 'queued',
      progress    REAL NOT NULL DEFAULT 0,
      speed       INTEGER NOT NULL DEFAULT 0,
      error       TEXT NOT NULL DEFAULT '',
      target_dir  TEXT NOT NULL DEFAULT '',
      metadata    TEXT NOT NULL DEFAULT '',
      created_at  TEXT NOT NULL,
      updated_at  TEXT NOT NULL,
      finished_at TEXT
    );

    CREATE TABLE IF NOT EXISTS settings (
      key        TEXT PRIMARY KEY,
      value      TEXT NOT NULL DEFAULT '',
      updated_at TEXT NOT NULL
    );
    "#,
    )
    .expect("建表失败");
    // 旧库迁移：tasks 表补 metadata 列
    let has_metadata: bool = db
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name = 'metadata'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(true);
    if !has_metadata {
        db.execute(
            "ALTER TABLE tasks ADD COLUMN metadata TEXT NOT NULL DEFAULT ''",
            [],
        )
        .ok();
    }
    db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 旧任务表自动补齐_metadata列() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    gopeed_id TEXT NOT NULL DEFAULT '',
                    source TEXT NOT NULL DEFAULT 'url',
                    source_url TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL DEFAULT 'queued',
                    progress REAL NOT NULL DEFAULT 0,
                    speed INTEGER NOT NULL DEFAULT 0,
                    error TEXT NOT NULL DEFAULT '',
                    target_dir TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    finished_at TEXT
                );",
            )
            .unwrap();
        legacy
            .execute(
                "INSERT INTO tasks (created_at, updated_at) VALUES ('2026-01-01', '2026-01-01')",
                [],
            )
            .unwrap();
        drop(legacy);

        let db = open_db(&path);
        let metadata_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name = 'metadata'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(metadata_count, 1);
        let metadata_default: String = db
            .query_row("SELECT metadata FROM tasks WHERE id = -1", [], |row| {
                row.get(0)
            })
            .unwrap_or_default();
        assert_eq!(metadata_default, "");
    }
}
