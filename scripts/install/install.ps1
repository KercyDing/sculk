# 检查执行策略
$executionPolicy = Get-ExecutionPolicy -Scope CurrentUser
if ($executionPolicy -eq "Restricted" -or $executionPolicy -eq "Undefined") {
    Write-Host "错误：当前系统已禁用 PowerShell 脚本执行。" -ForegroundColor Red
    Write-Host ""
    Write-Host "如需允许执行脚本，请运行以下命令：" -ForegroundColor Yellow
    Write-Host "  Set-ExecutionPolicy RemoteSigned -Scope CurrentUser" -ForegroundColor White
    Write-Host ""
    Write-Host "然后重新运行本安装脚本。" -ForegroundColor Yellow
    exit 1
}

$ErrorActionPreference = "Stop"

Write-Host "请选择要安装的组件：" -ForegroundColor Cyan
Write-Host "  1) sculk"
Write-Host "  2) sculk-tui"
Write-Host "  3) 全部 (默认)"
$choice = Read-Host "输入选项 [1/2/3]"
if ([string]::IsNullOrWhiteSpace($choice)) {
    $choice = "3"
}

$components = switch ($choice) {
    "1" { @("sculk") }
    "2" { @("sculk-tui") }
    "3" { @("sculk", "sculk-tui") }
    default {
        Write-Host "错误：无效选项 '$choice'" -ForegroundColor Red
        exit 1
    }
}

$INSTALL_DIR = "$env:LOCALAPPDATA\sculk"
New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null

foreach ($component in $components) {
    $artifact = if ($component -eq "sculk") {
        "sculk-windows-amd64.exe"
    } else {
        "sculk-tui-windows-amd64.exe"
    }
    $downloadUrl = "https://github.com/KercyDing/sculk/releases/latest/download/$artifact"
    $installPath = Join-Path $INSTALL_DIR "$component.exe"

    Write-Host "正在下载 $artifact..." -ForegroundColor Green
    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $installPath -UseBasicParsing
    } catch {
        Write-Host "错误：下载 $artifact 失败" -ForegroundColor Red
        exit 1
    }
    Write-Host "$component 已安装到 $installPath" -ForegroundColor Green
}

# 如未存在则添加到 PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$INSTALL_DIR*") {
    Write-Host "正在将 $INSTALL_DIR 添加到 PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$INSTALL_DIR", "User")
    $env:Path = "$env:Path;$INSTALL_DIR"
    Write-Host "PATH 已更新。" -ForegroundColor Green
}

Write-Host ""
foreach ($component in $components) {
    try {
        $version = & $component --version
        Write-Host "验证 ($component): $version" -ForegroundColor Green
    } catch {
        Write-Host "$component 已安装，请重启终端后使用。" -ForegroundColor Cyan
    }
}
Write-Host ""
Write-Host "安装完成。" -ForegroundColor Green
Write-Host ""
