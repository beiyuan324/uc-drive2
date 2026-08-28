//! 文件服务：文件系统为事实源，SQLite 只存元数据缓存。
//! 对标现有文件服务行为，保持文件树和 HTTP DTO 契约不变。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};

use super::error::{ApiError, ApiResult};
use super::models::{FileDto, ParentRef, TreeNode};
use super::util::{
    assert_inside, basename_of, mime_of, norm_path, now_iso, relative_suffix, scan_dir,
    unique_path, walk_files,
};
use super::winutil::CREATE_NO_WINDOW;

#[derive(Debug, Clone)]
pub struct FileRow {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub is_dir: bool,
    pub path: String,
    pub size: i64,
    pub mime: String,
    pub created_at: String,
    pub updated_at: String,
}

impl FileRow {
    pub fn dto(&self) -> FileDto {
        FileDto {
            id: self.id,
            name: self.name.clone(),
            parent_id: self.parent_id,
            is_dir: self.is_dir,
            path: self.path.clone(),
            size: self.size,
            mime: self.mime.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

fn db_error(err: rusqlite::Error) -> ApiError {
    ApiError::internal(format!("数据库错误: {err}"))
}

fn io_error(err: std::io::Error) -> ApiError {
    ApiError::from_io(&err)
}

pub fn get_row(db: &Connection, id: i64) -> Option<FileRow> {
    db.query_row(
        "SELECT id, name, parent_id, is_dir, path, size, mime, created_at, updated_at
         FROM files WHERE id = ?1",
        [id],
        row_from_sql,
    )
    .optional()
    .ok()
    .flatten()
}

fn row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileRow> {
    Ok(FileRow {
        id: row.get(0)?,
        name: row.get(1)?,
        parent_id: row.get(2)?,
        is_dir: row.get::<_, i64>(3)? != 0,
        path: row.get(4)?,
        size: row.get(5)?,
        mime: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn child_rows(db: &Connection, parent: &ParentRef) -> ApiResult<Vec<FileRow>> {
    let mut out = Vec::new();
    let mut stmt = match parent {
        ParentRef::Root => db
            .prepare(
                "SELECT id, name, parent_id, is_dir, path, size, mime, created_at, updated_at
                 FROM files WHERE parent_id IS NULL",
            )
            .map_err(db_error)?,
        ParentRef::Id(_id) => db
            .prepare(
                "SELECT id, name, parent_id, is_dir, path, size, mime, created_at, updated_at
                 FROM files WHERE parent_id = ?1",
            )
            .map_err(db_error)?,
        ParentRef::Invalid => return Err(ApiError::not_found("目录不存在")),
    };
    let rows = match parent {
        ParentRef::Root => stmt.query_map([], row_from_sql),
        ParentRef::Id(id) => stmt.query_map([id], row_from_sql),
        ParentRef::Invalid => unreachable!(),
    }
    .map_err(db_error)?;
    for row in rows {
        out.push(row.map_err(db_error)?);
    }
    Ok(out)
}

/// 由 id 解析磁盘绝对路径；Root 返回存储根，并自动创建目录。
pub fn resolve_dir_path(db: &Connection, storage: &Path, parent: &ParentRef) -> ApiResult<PathBuf> {
    let path = match parent {
        ParentRef::Root => storage.to_path_buf(),
        ParentRef::Id(id) => {
            let row = get_row(db, *id);
            if row.as_ref().map(|r| r.is_dir).unwrap_or(false) {
                PathBuf::from(row.unwrap().path)
            } else {
                return Err(ApiError::not_found("目录不存在"));
            }
        }
        ParentRef::Invalid => return Err(ApiError::not_found("目录不存在")),
    };
    std::fs::create_dir_all(&path).map_err(io_error)?;
    Ok(path)
}

/// 列目录：扫描磁盘并同步缓存。
pub fn list_dir(db: &Connection, storage: &Path, parent: &ParentRef) -> ApiResult<Vec<FileDto>> {
    let dir_path = resolve_dir_path(db, storage, parent)?;
    if !dir_path.exists() {
        prune_missing(db, &dir_path);
        return Ok(Vec::new());
    }

    let offline = norm_path(&storage.join("offline"));
    let entries = scan_dir(&dir_path)
        .into_iter()
        .filter(|entry| norm_path(&dir_path.join(&entry.name)) != offline)
        .collect::<Vec<_>>();
    let existing = child_rows(db, parent)?;
    let parent_id = match parent {
        ParentRef::Root => None,
        ParentRef::Id(id) => Some(*id),
        ParentRef::Invalid => return Err(ApiError::not_found("目录不存在")),
    };
    let mut upsert = db
        .prepare(
            "INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
               name = excluded.name,
               is_dir = excluded.is_dir,
               size = excluded.size,
               mime = excluded.mime,
               updated_at = excluded.updated_at
             WHERE files.name != excluded.name
                OR files.is_dir != excluded.is_dir
                OR files.size != excluded.size
                OR files.mime != excluded.mime",
        )
        .map_err(db_error)?;
    let ts = now_iso();
    for entry in &entries {
        let path = dir_path.join(&entry.name);
        let db_path = norm_path(&path);
        let mime = if entry.is_dir {
            ""
        } else {
            mime_of(&entry.name)
        };
        upsert
            .execute(params![
                entry.name,
                parent_id,
                if entry.is_dir { 1 } else { 0 },
                db_path,
                entry.size as i64,
                mime,
                ts,
                ts,
            ])
            .map_err(db_error)?;
    }
    drop(upsert);

    // FK ON DELETE CASCADE 会同时清掉缺失目录的递归后代。
    let mut delete = db
        .prepare("DELETE FROM files WHERE id = ?1")
        .map_err(db_error)?;
    for row in existing {
        if !entries_path_contains(&dir_path, &row.path, &entries) {
            delete.execute([row.id]).map_err(db_error)?;
        }
    }
    drop(delete);
    Ok(child_rows(db, parent)?
        .into_iter()
        .map(|r| r.dto())
        .collect())
}

fn entries_path_contains(
    dir: &Path,
    row_path: &str,
    entries: &[super::util::DirEntryInfo],
) -> bool {
    entries
        .iter()
        .any(|entry| norm_path(&dir.join(&entry.name)) == row_path)
}

fn prune_missing(db: &Connection, dir_path: &Path) {
    let dir = norm_path(dir_path);
    let like = format!("{dir}/%");
    let rows = db
        .prepare("SELECT id, path FROM files WHERE path = ?1 OR path LIKE ?2")
        .and_then(|mut stmt| {
            stmt.query_map(params![dir, like], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map(|mapped| mapped.filter_map(Result::ok).collect::<Vec<_>>())
        })
        .unwrap_or_default();
    for (id, path) in rows {
        if !Path::new(&path).exists() {
            let _ = db.execute("DELETE FROM files WHERE id = ?1", [id]);
        }
    }
}

fn is_cross_device(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(17) || err.kind() == std::io::ErrorKind::CrossesDevices
}

/// 跨卷移动（跨卷时降级为复制+删除）。
fn move_path(src: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device(&err) => {
            std::fs::copy(src, dest)?;
            std::fs::remove_file(src)
        }
        Err(err) => Err(err),
    }
}

/// 迁移存储根目录，返回实际移动的文件数。
pub fn move_storage_dir(db: &Connection, from: &Path, to: &Path) -> ApiResult<i64> {
    let old_root = norm_path(from);
    let new_root = norm_path(to);
    if old_root.eq_ignore_ascii_case(&new_root) {
        return Ok(0);
    }
    std::fs::create_dir_all(to).map_err(io_error)?;
    let mut stmt = db
        .prepare(
            "SELECT id, name, parent_id, is_dir, path, size, mime, created_at, updated_at
             FROM files ORDER BY is_dir DESC, id ASC",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], row_from_sql)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    drop(stmt);

    let mut moved = 0i64;
    for row in rows {
        let Some(relative) = relative_suffix(&old_root, &row.path) else {
            continue;
        };
        let dest = if relative.is_empty() {
            to.to_path_buf()
        } else {
            to.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
        };
        if row.is_dir {
            std::fs::create_dir_all(&dest).map_err(io_error)?;
        } else if Path::new(&row.path).exists() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(io_error)?;
            }
            move_path(Path::new(&row.path), &dest).map_err(io_error)?;
            moved += 1;
        }
        db.execute(
            "UPDATE files SET path = ?1 WHERE id = ?2",
            params![norm_path(&dest), row.id],
        )
        .map_err(db_error)?;
    }
    Ok(moved)
}

fn valid_name(name: &str) -> bool {
    let count = name.chars().count();
    count >= 1
        && count <= 255
        && !name
            .chars()
            .any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

fn parent_id(parent: &ParentRef) -> ApiResult<Option<i64>> {
    match parent {
        ParentRef::Root => Ok(None),
        ParentRef::Id(id) => Ok(Some(*id)),
        ParentRef::Invalid => Err(ApiError::not_found("目录不存在")),
    }
}

pub fn mkdir(
    db: &Connection,
    storage: &Path,
    name: &str,
    parent: &ParentRef,
) -> ApiResult<FileDto> {
    let parent_path = resolve_dir_path(db, storage, parent)?;
    if !valid_name(name) {
        return Err(ApiError::bad_request("目录名不合法"));
    }
    let dir_path = unique_path(&parent_path.join(name));
    std::fs::create_dir(&dir_path).map_err(io_error)?;
    let pid = parent_id(parent)?;
    let ts = now_iso();
    let path = norm_path(&dir_path);
    let actual_name = dir_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let inserted = db
        .execute(
            "INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
             VALUES (?1, ?2, 1, ?3, 0, '', ?4, ?5)",
            params![actual_name, pid, path, ts, ts],
        )
        .map_err(db_error)?;
    let _ = inserted;
    get_row(db, db.last_insert_rowid())
        .map(|r| r.dto())
        .ok_or_else(|| ApiError::internal("目录登记失败"))
}

#[derive(Debug, Clone)]
pub struct RenamePatch {
    pub name: Option<String>,
    /// None = 未提供 parent；Some(None) = 根目录；Some(Some(id)) = 指定目录
    pub parent: Option<Option<i64>>,
}

pub fn rename(db: &Connection, storage: &Path, id: i64, patch: RenamePatch) -> ApiResult<FileDto> {
    let row = get_row(db, id).ok_or_else(|| ApiError::not_found("文件不存在"))?;
    let mut target_dir = PathBuf::from(
        Path::new(&row.path)
            .parent()
            .unwrap_or_else(|| Path::new(".")),
    );
    if let Some(parent) = patch.parent {
        if row.is_dir {
            if let Some(parent_id) = parent {
                let descendants = collect_descendants(db, row.id)?;
                if parent_id == row.id
                    || descendants.iter().any(|(desc_id, _)| *desc_id == parent_id)
                {
                    return Err(ApiError::bad_request("不能移动到自身或其子目录"));
                }
            }
        }
        let parent_ref = match parent {
            Some(parent_id) => ParentRef::Id(parent_id),
            None => ParentRef::Root,
        };
        target_dir = resolve_dir_path(db, storage, &parent_ref)?;
    }
    let name = patch.name.unwrap_or_else(|| row.name.clone());
    if !valid_name(&name) {
        return Err(ApiError::bad_request("名称不合法"));
    }
    let new_path = unique_path(&target_dir.join(&name));
    assert_inside(storage, &new_path)?;
    std::fs::rename(Path::new(&row.path), &new_path).map_err(io_error)?;
    let new_db_path = norm_path(&new_path);
    let new_parent = match patch.parent {
        None => row.parent_id,
        Some(None) => None,
        Some(Some(parent_id)) => Some(parent_id),
    };
    db.execute(
        "UPDATE files SET name = ?1, parent_id = ?2, path = ?3, updated_at = ?4 WHERE id = ?5",
        params![name, new_parent, new_db_path, now_iso(), row.id],
    )
    .map_err(db_error)?;
    if row.is_dir && row.path != new_db_path {
        for (desc_id, desc_path) in collect_descendants(db, row.id)? {
            if let Some(relative) = desc_path.strip_prefix(&row.path) {
                let child_path = format!("{}{}", new_db_path, relative);
                db.execute(
                    "UPDATE files SET path = ?1 WHERE id = ?2",
                    params![child_path, desc_id],
                )
                .map_err(db_error)?;
            }
        }
    }
    get_row(db, row.id)
        .map(|r| r.dto())
        .ok_or_else(|| ApiError::internal("文件登记失败"))
}

fn collect_descendants(db: &Connection, id: i64) -> ApiResult<Vec<(i64, String)>> {
    let mut result = Vec::new();
    let mut stmt = db
        .prepare("SELECT id, path FROM files WHERE parent_id = ?1")
        .map_err(db_error)?;
    let rows = stmt
        .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    drop(stmt);
    for (child_id, path) in rows {
        result.push((child_id, path));
        result.extend(collect_descendants(db, child_id)?);
    }
    Ok(result)
}

fn busy_io(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
    ) || matches!(err.raw_os_error(), Some(16 | 32 | 33))
}

async fn move_to_recycle_bin(target: &Path) -> std::io::Result<()> {
    let escaped = target
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; $p = '{}'; if (Test-Path -LiteralPath $p) {{ $item = Get-Item -LiteralPath $p; if ($item.PSIsContainer) {{ [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p, 'OnlyErrorDialogs', 'SendToRecycleBin') }} else {{ [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($p, 'OnlyErrorDialogs', 'SendToRecycleBin') }} }}",
        escaped
    );
    let mut command = tokio::process::Command::new("powershell.exe");
    command.args(["-NoProfile", "-STA", "-Command", &script]);
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "回收站操作超时"))??;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn remove_permanently(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// 删除文件/目录：优先进回收站，回收站失败时永久删除。
/// db 只在 await 前后短暂加锁，避免不可 Send 的 rusqlite 引用跨 await。
pub async fn remove(db: &Mutex<Connection>, storage: &Path, id: i64) -> ApiResult<()> {
    let row = {
        let conn = db.lock().unwrap();
        get_row(&conn, id).ok_or_else(|| ApiError::not_found("文件不存在"))?
    };
    assert_inside(storage, Path::new(&row.path))?;
    if move_to_recycle_bin(Path::new(&row.path)).await.is_err() {
        if let Err(err) = remove_permanently(Path::new(&row.path)) {
            if busy_io(&err) {
                tokio::time::sleep(Duration::from_millis(300)).await;
                if let Err(err2) = remove_permanently(Path::new(&row.path)) {
                    if busy_io(&err2) {
                        return Err(ApiError {
                            status: 400,
                            message: "文件正被占用（可能正在预览或下载中），请稍后重试".to_string(),
                            kind: None,
                        });
                    }
                    return Err(io_error(err2));
                }
            } else {
                return Err(io_error(err));
            }
        }
    }
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM files WHERE id = ?1", [row.id])
        .map_err(db_error)?;
    Ok(())
}

/// 上传文件落盘并登记。跨卷时支持复制+删除。
pub fn register_upload(
    db: &Connection,
    storage: &Path,
    parent: &ParentRef,
    tmp_path: &Path,
    original_name: &str,
) -> ApiResult<FileDto> {
    let parent_path = resolve_dir_path(db, storage, parent)?;
    let original_name = basename_of(original_name);
    let dest = unique_path(&parent_path.join(&original_name));
    assert_inside(storage, &dest)?;
    move_path(tmp_path, &dest).map_err(io_error)?;
    let stat = std::fs::metadata(&dest).map_err(io_error)?;
    let pid = parent_id(parent)?;
    let ts = now_iso();
    db.execute(
        "INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7)",
        params![
            dest.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(original_name),
            pid,
            norm_path(&dest),
            stat.len() as i64,
            mime_of(&dest.to_string_lossy()),
            ts,
            ts,
        ],
    )
    .map_err(db_error)?;
    get_row(db, db.last_insert_rowid())
        .map(|r| r.dto())
        .ok_or_else(|| ApiError::internal("文件登记失败"))
}

pub fn search(db: &Connection, query: &str, limit: i64) -> ApiResult<Vec<FileDto>> {
    let like = format!("%{query}%");
    let mut stmt = db
        .prepare(
            "SELECT id, name, parent_id, is_dir, path, size, mime, created_at, updated_at
             FROM files WHERE name LIKE ?1 ORDER BY is_dir DESC, name LIMIT ?2",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![like, limit], row_from_sql)
        .map_err(db_error)?;
    rows.map(|row| row.map(|r| r.dto()).map_err(db_error))
        .collect()
}

pub fn tree(db: &Connection) -> ApiResult<Vec<TreeNode>> {
    let mut stmt = db
        .prepare("SELECT id, name, path, parent_id FROM files WHERE is_dir = 1 ORDER BY name")
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    drop(stmt);
    fn build(parent: Option<i64>, rows: &[(i64, String, String, Option<i64>)]) -> Vec<TreeNode> {
        rows.iter()
            .filter(|(_, _, _, p)| *p == parent)
            .map(|(id, name, path, _)| TreeNode {
                id: *id,
                name: name.clone(),
                path: path.clone(),
                children: build(Some(*id), rows),
            })
            .collect()
    }
    Ok(build(None, &rows))
}

pub fn ancestors(db: &Connection, id: i64) -> ApiResult<Option<Vec<FileDto>>> {
    let mut out = Vec::new();
    let mut row = get_row(db, id);
    if row.is_none() {
        return Ok(None);
    }
    while let Some(current) = row {
        let parent_id = current.parent_id;
        out.push(current.dto());
        row = parent_id.and_then(|parent| get_row(db, parent));
    }
    out.reverse();
    Ok(Some(out))
}

/// 任务完成后登记文件时使用：确保父链存在并 upsert 文件行。
pub fn upsert_file_row(
    db: &Connection,
    storage: &Path,
    initial_parent_id: Option<i64>,
    abs_path: &Path,
) -> ApiResult<()> {
    let stat = std::fs::metadata(abs_path).map_err(io_error)?;
    let db_path = norm_path(abs_path);
    let mut parent_id = initial_parent_id;
    let storage_norm = norm_path(storage);
    let parent_dir = abs_path.parent().unwrap_or_else(|| Path::new("."));
    if let Some(relative) = relative_suffix(&storage_norm, &norm_path(parent_dir)) {
        if !relative.is_empty() {
            let mut current = storage.to_path_buf();
            for part in relative.split('/').filter(|part| !part.is_empty()) {
                current.push(part);
                let current_norm = norm_path(&current);
                let existing: Option<i64> = db
                    .query_row(
                        "SELECT id FROM files WHERE path = ?1",
                        [&current_norm],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(db_error)?;
                parent_id = if let Some(existing) = existing {
                    Some(existing)
                } else {
                    let ts = now_iso();
                    db.execute(
                        "INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
                         VALUES (?1, ?2, 1, ?3, 0, '', ?4, ?5)",
                        params![part, parent_id, current_norm, ts, ts],
                    )
                    .map_err(db_error)?;
                    Some(db.last_insert_rowid())
                };
            }
        }
    }
    let name = abs_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ts = now_iso();
    db.execute(
        "INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(path) DO UPDATE SET
           name = excluded.name, is_dir = excluded.is_dir, size = excluded.size,
           mime = excluded.mime, updated_at = excluded.updated_at",
        params![
            name,
            parent_id,
            if stat.is_dir() { 1 } else { 0 },
            db_path,
            if stat.is_dir() { 0 } else { stat.len() as i64 },
            if stat.is_dir() { "" } else { mime_of(&db_path) },
            ts,
            ts,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

/// 任务登记使用：扫描 target_dir 下的普通文件。
pub fn collect_download_files(target_dir: &Path) -> Vec<PathBuf> {
    walk_files(target_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 名称校验和唯一化() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(unique_path(&p).file_name().unwrap(), "a (1).txt");
        assert!(!valid_name("a/b"));
        assert!(valid_name("中文文件"));
    }
}
