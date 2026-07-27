# 下载、安装与卸载

## CLI（`sculk`）

### 方式一：一键脚本（推荐）

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.ps1)))
```

### 方式二：Arch Linux（AUR，`paru` 示例）

```sh
# 稳定版（推荐）
paru -S sculk-bin

# 开发版（跟随 main 分支）
paru -S sculk-git
```

### 方式三：从 crates.io 安装

```sh
cargo install sculk-cli
```

### 方式四：从源码安装

```sh
git clone https://github.com/KercyDing/sculk.git
cd sculk

cargo install --path cli
```

## 卸载

### 方式一：一键脚本（推荐）

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/uninstall/uninstall.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/uninstall/uninstall.ps1)))
```

### 方式二：Cargo 卸载

```sh
cargo uninstall sculk-cli
```

注意：二进制名是 `sculk`，Cargo 包名是 `sculk-cli`。卸载脚本也会清理旧版的 `sckc` 和 `sckt` 二进制。
