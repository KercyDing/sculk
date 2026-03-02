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
Write-Host "  1) sckc(sculk-cli)"
Write-Host "  2) sckt(sculk-tui)"
Write-Host "  3) 全部 (默认)"
$choice = Read-Host "输入选项 [1/2/3]"
if ([string]::IsNullOrWhiteSpace($choice)) {
    $choice = "3"
}

$components = switch ($choice) {
    "1" { @("sckc") }
    "2" { @("sckt") }
    "3" { @("sckc", "sckt") }
    default {
        Write-Host "错误：无效选项 '$choice'" -ForegroundColor Red
        exit 1
    }
}

$INSTALL_DIR = "$env:LOCALAPPDATA\sculk"
New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null

$installed = 0

foreach ($component in $components) {
    $cargoPath = Join-Path "$env:USERPROFILE\.cargo\bin" "$component.exe"
    if (Test-Path $cargoPath) {
        $pkgName = if ($component -eq "sckc") { "sculk-cli" } else { "sculk-tui" }
        Write-Host "警告：检测到 $cargoPath，建议先执行 cargo uninstall $pkgName 避免冲突。" -ForegroundColor Yellow
        $answer = Read-Host "是否继续安装？[y/N]"
        if ($answer -notmatch '^[yY]') {
            Write-Host "已跳过 $component" -ForegroundColor Cyan
            continue
        }
        $delAnswer = Read-Host "是否删除 $cargoPath？[y/N]"
        if ($delAnswer -match '^[yY]') {
            Remove-Item -Path $cargoPath -Force
            Write-Host "已删除 $cargoPath" -ForegroundColor Green
        }
    }

    $artifact = if ($component -eq "sckc") {
        "sckc-windows-amd64.exe"
    } else {
        "sckt-windows-amd64.exe"
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
    $installed++
}

if ($installed -eq 0) {
    Write-Host ""
    Write-Host "未安装任何组件。"
    exit 0
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
    if ($component -eq "sckt") {
        $exePath = Join-Path $INSTALL_DIR "$component.exe"
        if (Test-Path $exePath) {
            Write-Host "验证 (sckt/sculk-tui): 已安装到 $exePath" -ForegroundColor Green
        }
    } else {
        try {
            $version = & $component --version
            Write-Host "验证 (sckc/sculk-cli): $version" -ForegroundColor Green
        } catch {
            Write-Host "$component 已安装，请重启终端后使用。" -ForegroundColor Cyan
        }
    }
}
Write-Host ""
Write-Host "安装完成。" -ForegroundColor Green
Write-Host ""
