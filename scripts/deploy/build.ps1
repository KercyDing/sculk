# 交叉编译 iroh-relay（Windows 下通过 WSL 调用 build.sh）。
# 用法: build.ps1 [-Target amd64|arm64|all]
param(
    [string]$Target = "all"
)

if (-not (Get-Command wsl -ErrorAction SilentlyContinue)) {
    Write-Error "未找到 WSL。请安装 WSL 后重试，或在 Linux/macOS 上直接运行 build.sh。"
    exit 1
}

wsl bash scripts/deploy/build.sh $Target
