# 下载与安装

## CLI（`sckc`）

### 方式一：一键脚本（推荐）

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.ps1)))
```

脚本会交互式询问安装项，请选择 `sckc`（或“全部”）。

### 方式二：Arch Linux（AUR，`paru` 示例）

```sh
# 稳定版（推荐）
paru -S sculk-cli-bin

# 开发版（跟随 main 分支）
paru -S sculk-cli-git
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

## TUI（`sckt`）

### 方式一：一键脚本（推荐）

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.ps1)))
```

脚本会交互式询问安装项，请选择 `sckt`（或“全部”）。

### 方式二：Arch Linux（AUR，`paru` 示例）

```sh
# 稳定版（推荐）
paru -S sculk-tui-bin

# 开发版（跟随 main 分支）
paru -S sculk-tui-git
```

### 方式三：从 crates.io 安装

```sh
cargo install sculk-tui
```

### 方式四：从源码安装

```sh
git clone https://github.com/KercyDing/sculk.git
cd sculk

cargo install --path tui
```
