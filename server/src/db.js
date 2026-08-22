import { DatabaseSync } from 'node:sqlite';
import fs from 'node:fs';
import path from 'node:path';
import { DB_FILE } from './config.js';

/**
 * 打开 SQLite（Node 内置 node:sqlite，无原生模块依赖，便于 sidecar 打包）。
 * 返回 DatabaseSync 实例。所有语句同步执行。
 */
export function openDb(file = DB_FILE) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const db = new DatabaseSync(file);
  db.exec('PRAGMA journal_mode = WAL;');
  db.exec('PRAGMA foreign_keys = ON;');
  db.exec(`
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
  `);
  // 旧库迁移：tasks 表补 metadata 列（target_dir 无默认值的旧表）
  const cols = db.prepare(`PRAGMA table_info(tasks)`).all().map(c => c.name);
  if (!cols.includes('metadata')) {
    db.exec(`ALTER TABLE tasks ADD COLUMN metadata TEXT NOT NULL DEFAULT ''`);
  }
  return db;
}
