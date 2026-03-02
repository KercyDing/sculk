//! 密钥文件管理：加载、生成并持久化 iroh `SecretKey`。

use std::path::Path;

use crate::Result;
use crate::error::PersistError;
use crate::tunnel::SecretKey;

const KEY_LEN: usize = 32;

/// 从文件加载密钥；若文件不存在则生成新密钥并保存。
pub fn load_or_generate_key(path: &Path) -> Result<SecretKey> {
    if path.exists() {
        load_key(path)
    } else {
        generate_new_key(path)
    }
}

/// 强制重新生成新密钥并保存。
pub fn generate_new_key(path: &Path) -> Result<SecretKey> {
    let bytes: [u8; KEY_LEN] = rand::random();
    let key = SecretKey::from_bytes(&bytes);
    save_key(path, &key)?;
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

fn save_key(path: &Path, key: &SecretKey) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PersistError::PathIo {
            op: "create key directory",
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| PersistError::PathIo {
                op: "open key file",
                path: path.to_path_buf(),
                source: e,
            })?;
        file.write_all(&key.to_bytes())
            .map_err(|e| PersistError::PathIo {
                op: "write key file",
                path: path.to_path_buf(),
                source: e,
            })?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, key.to_bytes()).map_err(|e| PersistError::PathIo {
            op: "write key file",
            path: path.to_path_buf(),
            source: e,
        })?;
    }

    Ok(())
}
