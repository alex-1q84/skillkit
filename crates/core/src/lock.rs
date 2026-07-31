//! 文件锁：CLI/server 并发写 ~/.skillkit/ 状态文件时串行化。粒度到单文件，读不锁。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use fs2::FileExt;
use std::fs::OpenOptions;
use std::time::{Duration, Instant};

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// 排他文件锁 guard。持有期间排他，drop 自动释放。
pub struct FileLock {
    file: std::fs::File,
}

impl FileLock {
    /// 默认 5s 超时（写操作用）。
    pub fn acquire(paths: &Paths, key: &str) -> Result<Self> {
        Self::acquire_with_timeout(paths, key, LOCK_TIMEOUT)
    }

    /// 带自定义超时（测试用短超时）。轮询 try_lock_exclusive，超时报 LockTimeout。
    pub fn acquire_with_timeout(paths: &Paths, key: &str, timeout: Duration) -> Result<Self> {
        let lock_dir = paths.skillkit_dir().join(".lock");
        std::fs::create_dir_all(&lock_dir)?;
        let lock_path = lock_dir.join(format!("{key}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .read(true)
            .open(&lock_path)?;
        let deadline = Instant::now() + timeout;
        loop {
            if file.try_lock_exclusive().is_ok() {
                return Ok(FileLock { file });
            }
            if Instant::now() >= deadline {
                return Err(SkillkitError::LockTimeout {
                    key: key.to_string(),
                });
            }
            std::thread::sleep(RETRY_INTERVAL);
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_acquires_same_key_after_release() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().to_path_buf());
        let l1 =
            FileLock::acquire_with_timeout(&paths, "registry", Duration::from_secs(1)).unwrap();
        drop(l1);
        // 释放后同 key 能再获
        let _l2 =
            FileLock::acquire_with_timeout(&paths, "registry", Duration::from_secs(1)).unwrap();
    }
}
