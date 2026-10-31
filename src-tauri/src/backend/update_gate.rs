//! 请求和后台任务共用的安装闸门，检查空闲与禁止新写入在同一把锁内完成。
use std::sync::Arc;

use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

#[derive(Clone, Default)]
pub struct UpdateGate(Arc<RwLock<()>>);

impl UpdateGate {
    pub fn enter(&self) -> Result<OwnedRwLockReadGuard<()>, String> {
        self.0
            .clone()
            .try_read_owned()
            .map_err(|_| "正在安装更新，请稍后再试".to_string())
    }

    pub fn begin_install(&self) -> Result<OwnedRwLockWriteGuard<()>, String> {
        self.0
            .clone()
            .try_write_owned()
            .map_err(|_| "仍有上传、文件写入或任务同步正在处理，请稍后安装".to_string())
    }
}
