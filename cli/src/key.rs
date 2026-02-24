//! 密钥文件管理：加载、生成、持久化 iroh SecretKey。

use std::path::Path;

use anyhow::{Context, ensure};
use sculk_core::tunnel::SecretKey;

const KEY_LEN: usize = 32;

/// 从文件加载密钥；文件不存在则生成新密钥并保存。
pub fn load_or_generate_key(path: &Path) -> anyhow::Result<SecretKey> {
    if path.exists() {
        load_key(path)
    } else {
        generate_new_key(path)
    }
}

/// 强制生成新密钥并保存（覆盖已有文件）。
pub fn generate_new_key(path: &Path) -> anyhow::Result<SecretKey> {
    let key = SecretKey::generate(&mut rand::rng());
    save_key(path, &key)?;
    Ok(key)
}

fn load_key(path: &Path) -> anyhow::Result<SecretKey> {
    let bytes =
        std::fs::read(path).with_context(|| format!("读取密钥文件失败: {}", path.display()))?;
    ensure!(
        bytes.len() == KEY_LEN,
        "密钥文件长度错误: 期望 {KEY_LEN} 字节，实际 {} 字节",
        bytes.len()
    );
    let arr: [u8; KEY_LEN] = bytes.try_into().unwrap();
    Ok(SecretKey::from_bytes(&arr))
}

fn save_key(path: &Path, key: &SecretKey) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建密钥目录失败: {}", parent.display()))?;
    }
    std::fs::write(path, key.to_bytes())
        .with_context(|| format!("写入密钥文件失败: {}", path.display()))?;
    Ok(())
}
