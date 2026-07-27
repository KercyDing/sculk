$ErrorActionPreference = "Stop"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\Sculk"
$removed = $false

foreach ($name in @("sculk.exe", "sckc.exe", "sckt.exe")) {
    $path = Join-Path $installDir $name
    if (Test-Path $path) {
        Remove-Item -Path $path -Force
        Write-Host "已删除：$path" -ForegroundColor Green
        $removed = $true
    }
}

$cargoDir = Join-Path "$env:USERPROFILE\.cargo" "bin"
if ((Test-Path (Join-Path $cargoDir "sculk.exe")) -or
    (Test-Path (Join-Path $cargoDir "sckc.exe"))) {
    Write-Host "检测到 Cargo 安装版本，请执行：cargo uninstall sculk-cli" -ForegroundColor Yellow
    $removed = $true
}
if (Test-Path (Join-Path $cargoDir "sckt.exe")) {
    Write-Host "检测到旧版 TUI，请执行：cargo uninstall sculk-tui" -ForegroundColor Yellow
    $removed = $true
}

if (Test-Path $installDir) {
    $remaining = Get-ChildItem -Path $installDir -Force -ErrorAction SilentlyContinue
    if (-not $remaining) {
        Remove-Item -Path $installDir -Force
    }
}

if (-not $removed) {
    Write-Host "常见路径中未找到 sculk。" -ForegroundColor Yellow
}
