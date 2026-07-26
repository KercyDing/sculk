//! 密钥文件管理：加载、生成并持久化 iroh `SecretKey`。

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::error::PersistError;
use crate::types::SecretKey;

const KEY_LEN: usize = 32;
const TEMP_CREATE_ATTEMPTS_MAX: usize = 16;

/// 从文件加载密钥；若文件不存在则原子生成新密钥并保存。
///
/// 通过先加载、失败时原子创建的顺序避免 TOCTOU 竞态。
pub fn load_or_generate_key(path: &Path) -> Result<SecretKey> {
    match load_key(path) {
        Ok(key) => Ok(key),
        Err(e) if is_not_found(&e) => generate_key_exclusive(path),
        Err(e) => Err(e),
    }
}

fn generate_key_exclusive(path: &Path) -> Result<SecretKey> {
    let bytes: [u8; KEY_LEN] = rand::random();
    let key = SecretKey::from_bytes(&bytes);
    match save_key_exclusive(path, &key) {
        Ok(()) => Ok(key),
        Err(e) if is_already_exists(&e) => load_key(path),
        Err(e) => Err(e),
    }
}

/// 强制重新生成新密钥并保存。
///
/// 新密钥先完整写入同目录临时文件，再原子替换目标文件。
///
/// 调用方不得并发执行密钥轮换。
pub fn generate_new_key(path: &Path) -> Result<SecretKey> {
    let bytes: [u8; KEY_LEN] = rand::random();
    let key = SecretKey::from_bytes(&bytes);
    save_key_replace(path, &key)?;
    Ok(key)
}

fn load_key(path: &Path) -> Result<SecretKey> {
    let bytes = std::fs::read(path).map_err(|e| PersistError::PathIo {
        op: "read key file",
        path: path.to_path_buf(),
        source: e,
    })?;
    if bytes.len() != KEY_LEN {
        return Err(PersistError::InvalidKeyLength {
            expected: KEY_LEN,
            actual: bytes.len(),
        }
        .into());
    }
    let arr: [u8; KEY_LEN] =
        bytes
            .try_into()
            .map_err(|v: Vec<u8>| PersistError::InvalidKeyLength {
                expected: KEY_LEN,
                actual: v.len(),
            })?;
    Ok(SecretKey::from_bytes(&arr))
}

/// 发布完整写入的密钥文件；目标已存在时加载已发布的密钥。
fn save_key_exclusive(path: &Path, key: &SecretKey) -> Result<()> {
    let temp = write_temp_key(path, key)?;
    match std::fs::hard_link(temp.path(), path) {
        Ok(()) => sync_parent(path),
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(PersistError::PathIo {
                op: "create key file",
                path: path.to_path_buf(),
                source,
            }
            .into())
        }
        Err(source) => Err(PersistError::PathIo {
            op: "publish key file",
            path: path.to_path_buf(),
            source,
        }
        .into()),
    }
}

/// 使用同目录 rename 原子替换现有密钥。
fn save_key_replace(path: &Path, key: &SecretKey) -> Result<()> {
    let temp = write_temp_key(path, key)?;
    std::fs::rename(temp.path(), path).map_err(|source| PersistError::PathIo {
        op: "replace key file",
        path: path.to_path_buf(),
        source,
    })?;
    sync_parent(path)
}

fn write_temp_key(path: &Path, key: &SecretKey) -> Result<TempKey> {
    let parent = parent_dir(path);
    std::fs::create_dir_all(parent).map_err(|source| PersistError::PathIo {
        op: "create key directory",
        path: parent.to_path_buf(),
        source,
    })?;

    for _ in 0..TEMP_CREATE_ATTEMPTS_MAX {
        let temp_path = temp_path(path);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = match options.open(&temp_path) {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(PersistError::PathIo {
                    op: "create temporary key file",
                    path: temp_path,
                    source,
                }
                .into());
            }
        };
        let temp = TempKey { path: temp_path };
        file.write_all(&key.to_bytes())
            .map_err(|source| PersistError::PathIo {
                op: "write temporary key file",
                path: temp.path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| PersistError::PathIo {
            op: "sync temporary key file",
            path: temp.path.clone(),
            source,
        })?;
        return Ok(temp);
    }

    Err(PersistError::PathIo {
        op: "create temporary key file",
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "temporary key file name collision limit reached",
        ),
    }
    .into())
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let parent = parent_dir(path);
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|source| PersistError::PathIo {
                op: "sync key directory",
                path: parent.to_path_buf(),
                source,
            })?;
    }

    #[cfg(not(unix))]
    let _ = path;

    Ok(())
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("secret.key")),
    );
    name.push(format!(".{:016x}.tmp", rand::random::<u64>()));
    parent_dir(path).join(name)
}

struct TempKey {
    path: PathBuf,
}

impl TempKey {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempKey {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 检查错误链中是否包含 `io::ErrorKind::NotFound`。
fn is_not_found(err: &crate::error::SculkError) -> bool {
    matches!(
        err,
        crate::error::SculkError::Persist(PersistError::PathIo { source, .. })
        if source.kind() == std::io::ErrorKind::NotFound
    )
}

/// 检查错误链中是否包含 `io::ErrorKind::AlreadyExists`。
fn is_already_exists(err: &crate::error::SculkError) -> bool {
    matches!(
        err,
        crate::error::SculkError::Persist(PersistError::PathIo { source, .. })
        if source.kind() == std::io::ErrorKind::AlreadyExists
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sculk_key_{name}_{}_{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    #[test]
    fn persists_key() {
        let dir = test_dir("persist");
        let path = dir.join("secret.key");

        let first = load_or_generate_key(&path);
        assert!(first.is_ok(), "initial key creation failed");
        let first = if let Ok(key) = first { key } else { return };
        let second = load_or_generate_key(&path);
        assert!(second.is_ok(), "persisted key load failed");
        let second = if let Ok(key) = second { key } else { return };

        assert_eq!(first.to_bytes(), second.to_bytes());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn replaces_key() {
        let dir = test_dir("replace");
        let path = dir.join("secret.key");

        let first = load_or_generate_key(&path);
        assert!(first.is_ok(), "initial key creation failed");
        let first = if let Ok(key) = first { key } else { return };
        let replacement = generate_new_key(&path);
        assert!(replacement.is_ok(), "key replacement failed");
        let replacement = if let Ok(key) = replacement {
            key
        } else {
            return;
        };
        let loaded = load_or_generate_key(&path);
        assert!(loaded.is_ok(), "replacement key load failed");
        let loaded = if let Ok(key) = loaded { key } else { return };

        assert_ne!(first.to_bytes(), replacement.to_bytes());
        assert_eq!(replacement.to_bytes(), loaded.to_bytes());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_short_key() {
        const TRUNCATED_LEN: usize = KEY_LEN - 1;

        let dir = test_dir("truncated");
        let path = dir.join("secret.key");
        let create_dir = std::fs::create_dir_all(&dir);
        assert!(create_dir.is_ok(), "test directory creation failed");
        let write = std::fs::write(&path, [0_u8; TRUNCATED_LEN]);
        assert!(write.is_ok(), "truncated key write failed");

        let result = load_or_generate_key(&path);
        assert!(matches!(
            result,
            Err(crate::error::SculkError::Persist(
                PersistError::InvalidKeyLength {
                    expected: KEY_LEN,
                    actual: TRUNCATED_LEN
                }
            ))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn concurrent_create_is_stable() {
        const THREAD_COUNT: usize = 8;

        let dir = test_dir("concurrent");
        let path = Arc::new(dir.join("secret.key"));
        let barrier = Arc::new(Barrier::new(THREAD_COUNT));
        let mut threads = Vec::with_capacity(THREAD_COUNT);

        for _ in 0..THREAD_COUNT {
            let path = path.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                load_or_generate_key(&path).map(|key| key.to_bytes())
            }));
        }

        let mut keys = Vec::with_capacity(THREAD_COUNT);
        for thread in threads {
            let joined = thread.join();
            assert!(joined.is_ok(), "key thread panicked");
            let result = if let Ok(result) = joined {
                result
            } else {
                return;
            };
            assert!(result.is_ok(), "concurrent key creation failed");
            if let Ok(key) = result {
                keys.push(key);
            } else {
                return;
            }
        }

        assert_eq!(keys.len(), THREAD_COUNT);
        assert!(keys.iter().all(|key| key == &keys[0]));
        let _ = std::fs::remove_dir_all(dir);
    }
}
