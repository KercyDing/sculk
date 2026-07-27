$executionPolicy = Get-ExecutionPolicy -Scope CurrentUser
if ($executionPolicy -eq "Restricted" -or $executionPolicy -eq "Undefined") {
    Write-Host "错误：当前系统已禁用 PowerShell 脚本执行。" -ForegroundColor Red
    Write-Host "请先运行：Set-ExecutionPolicy RemoteSigned -Scope CurrentUser" -ForegroundColor Yellow
    exit 1
}

$ErrorActionPreference = "Stop"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\Sculk"
$cargoPath = Join-Path "$env:USERPROFILE\.cargo\bin" "sculk.exe"

if (Test-Path $cargoPath) {
    Write-Host "警告：检测到 $cargoPath，建议先执行 cargo uninstall sculk-cli 避免冲突。" -ForegroundColor Yellow
    $answer = Read-Host "是否继续安装？[y/N]"
    if ($answer -notmatch '^[yY]') {
        exit 0
    }
}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
$artifact = "sculk-windows-amd64.exe"
$downloadUrl = "https://github.com/KercyDing/sculk/releases/latest/download/$artifact"
$installPath = Join-Path $installDir "sculk.exe"

Write-Host "正在下载 $artifact..." -ForegroundColor Green
Invoke-WebRequest -Uri $downloadUrl -OutFile $installPath -UseBasicParsing

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -split ";" -notcontains $installDir) {
    $newPath = if ([string]::IsNullOrEmpty($userPath)) {
        $installDir
    } else {
        "$userPath;$installDir"
    }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "已将 $installDir 添加到用户 PATH，请打开新终端后使用。" -ForegroundColor Yellow
}

Write-Host "sculk 已安装到 $installPath" -ForegroundColor Green
