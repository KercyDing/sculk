//! Host 的稳定服务标识和令牌状态持久化。

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::Result;
use crate::error::PersistError;
use crate::tunnel::{AccessToken, ServiceId, TokenState};

const STATE_VERSION: u8 = 1;
const STATE_LEN: usize = 1 + 16 + 32 + 8 + 4;
const TEMP_CREATE_ATTEMPTS_MAX: usize = 16;

/// 跨 Host 发布保存的稳定服务标识和令牌状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostState {
    pub service_id: ServiceId,
    pub token_state: TokenState,
}

/// 从文件加载 Host 状态；文件不存在时返回 `None`。
pub fn load_host_state(path: &Path) -> Result<Option<HostState>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(PersistError::PathIo {
                op: "read host state",
                path: path.to_path_buf(),
                source,
            }
            .into());
        }
    };
    if bytes.len() != STATE_LEN {
        return Err(PersistError::InvalidHostStateLength {
            expected: STATE_LEN,
            actual: bytes.len(),
        }
        .into());
    }
    if bytes[0] != STATE_VERSION {
        return Err(PersistError::UnsupportedHostStateVersion(bytes[0]).into());
    }

    let service_id = ServiceId::from_bytes(read_array::<16>(&bytes[1..17]));
    let token = AccessToken::from_bytes(read_array::<32>(&bytes[17..49]));
    let seconds = u64::from_le_bytes(read_array::<8>(&bytes[49..57]));
    let nanos = u32::from_le_bytes(read_array::<4>(&bytes[57..61]));
    if nanos >= 1_000_000_000 {
        return Err(PersistError::InvalidHostStateTimestamp.into());
    }
    let created_at = SystemTime::UNIX_EPOCH
        .checked_add(Duration::new(seconds, nanos))
        .ok_or(PersistError::InvalidHostStateTimestamp)?;
    Ok(Some(HostState {
        service_id,
        token_state: TokenState::new(token, created_at),
    }))
}

/// 原子保存 Host 状态，并在 Unix 上限制为当前用户可读写。
pub fn save_host_state(path: &Path, state: &HostState) -> Result<()> {
    let created = state
        .token_state
        .created_at()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| PersistError::InvalidHostStateTimestamp)?;
    let mut bytes = [0_u8; STATE_LEN];
    bytes[0] = STATE_VERSION;
    bytes[1..17].copy_from_slice(&state.service_id.to_bytes());
    bytes[17..49].copy_from_slice(&state.token_state.token().to_bytes());
    bytes[49..57].copy_from_slice(&created.as_secs().to_le_bytes());
    bytes[57..61].copy_from_slice(&created.subsec_nanos().to_le_bytes());
    write_atomic(path, &bytes)
}

fn read_array<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut result = [0; N];
    result.copy_from_slice(bytes);
    result
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|source| PersistError::PathIo {
        op: "create host state directory",
        path: parent.to_path_buf(),
        source,
    })?;

    for _ in 0..TEMP_CREATE_ATTEMPTS_MAX {
        let temp_path = temp_path(path, parent);
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
                    op: "create temporary host state",
                    path: temp_path,
                    source,
                }
                .into());
            }
        };
        let temp = TempState { path: temp_path };
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| PersistError::PathIo {
                op: "write temporary host state",
                path: temp.path.clone(),
                source,
            })?;
        std::fs::rename(&temp.path, path).map_err(|source| PersistError::PathIo {
            op: "replace host state",
            path: path.to_path_buf(),
            source,
        })?;
        sync_parent(parent)?;
        return Ok(());
    }

    Err(PersistError::PathIo {
        op: "create temporary host state",
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "temporary host state collision limit reached",
        ),
    }
    .into())
}

fn temp_path(path: &Path, parent: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("host.state")),
    );
    name.push(format!(".{:016x}.tmp", rand::random::<u64>()));
    parent.join(name)
}

struct TempState {
    path: PathBuf,
}

impl Drop for TempState {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn sync_parent(parent: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|source| PersistError::PathIo {
                op: "sync host state directory",
                path: parent.to_path_buf(),
                source,
            })?;
    }
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sculk_host_state_{name}_{}_{:016x}/host.state",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    #[test]
    fn missing_state_is_empty() {
        let path = test_path("missing");
        assert!(matches!(load_host_state(&path), Ok(None)));
    }

    #[test]
    fn state_roundtrips() {
        let path = test_path("roundtrip");
        let state = HostState {
            service_id: ServiceId::from_bytes([0x4b; 16]),
            token_state: TokenState::new(
                AccessToken::from_bytes([0x5a; 32]),
                SystemTime::UNIX_EPOCH + Duration::new(123_456, 789),
            ),
        };

        assert!(save_host_state(&path, &state).is_ok());
        assert_eq!(load_host_state(&path).ok().flatten(), Some(state));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode =
                std::fs::metadata(&path).map(|metadata| metadata.permissions().mode() & 0o777);
            assert!(matches!(mode, Ok(0o600)));
        }

        let _ = std::fs::remove_dir_all(path.parent().unwrap_or_else(|| Path::new(".")));
    }

    #[test]
    fn rejects_invalid_state() {
        let path = test_path("invalid");
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        assert!(std::fs::create_dir_all(parent).is_ok());
        assert!(std::fs::write(&path, [STATE_VERSION]).is_ok());
        assert!(matches!(
            load_host_state(&path),
            Err(crate::error::SculkError::Persist(
                PersistError::InvalidHostStateLength { .. }
            ))
        ));

        let mut bytes = [0_u8; STATE_LEN];
        bytes[0] = STATE_VERSION + 1;
        assert!(std::fs::write(&path, bytes).is_ok());
        assert!(matches!(
            load_host_state(&path),
            Err(crate::error::SculkError::Persist(
                PersistError::UnsupportedHostStateVersion(_)
            ))
        ));

        bytes[0] = STATE_VERSION;
        bytes[57..61].copy_from_slice(&1_000_000_000_u32.to_le_bytes());
        assert!(std::fs::write(&path, bytes).is_ok());
        assert!(matches!(
            load_host_state(&path),
            Err(crate::error::SculkError::Persist(
                PersistError::InvalidHostStateTimestamp
            ))
        ));
        let _ = std::fs::remove_dir_all(parent);
    }
}
