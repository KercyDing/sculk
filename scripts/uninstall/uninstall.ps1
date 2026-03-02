$ErrorActionPreference = "Stop"

Write-Host "请选择要卸载的组件：" -ForegroundColor Cyan
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
$removedAny = $false

foreach ($component in $components) {
    $path = Join-Path $INSTALL_DIR "$component.exe"
    if (Test-Path $path) {
        Remove-Item -Path $path -Force
        Write-Host "已删除：$path" -ForegroundColor Green
        $removedAny = $true
    } else {
        Write-Host "警告：在 $path 未找到 $component" -ForegroundColor Yellow
    }
    $cargoPath = Join-Path "$env:USERPROFILE\.cargo\bin" "$component.exe"
    if (Test-Path $cargoPath) {
        $pkgName = if ($component -eq "sculk") { "sculk-cli" } else { "sculk-tui" }
        Write-Host "检测到 $cargoPath，由 cargo install 安装，请手动执行：cargo uninstall $pkgName" -ForegroundColor Yellow
    }
}

if (Test-Path $INSTALL_DIR) {
    $remaining = Get-ChildItem -Path $INSTALL_DIR -File -ErrorAction SilentlyContinue
    if (-not $remaining) {
        Remove-Item -Path $INSTALL_DIR -Recurse -Force
        Write-Host "已删除空目录：$INSTALL_DIR" -ForegroundColor Green
    }
}

# 安装目录不存在时，从 PATH 中移除
if (-not (Test-Path $INSTALL_DIR)) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -like "*$INSTALL_DIR*") {
        Write-Host "正在从 PATH 移除 $INSTALL_DIR..." -ForegroundColor Cyan
        $pathArray = $userPath -split ';' | Where-Object { $_ -ne $INSTALL_DIR -and $_ -ne "" }
        $newPath = $pathArray -join ';'
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "已从 PATH 移除。" -ForegroundColor Green
    }
}

Write-Host ""
if ($removedAny) {
    Write-Host "卸载完成。" -ForegroundColor Green
} else {
    Write-Host "没有可卸载的内容。" -ForegroundColor Yellow
}
Write-Host "请重启终端以使 PATH 变更生效。" -ForegroundColor Cyan
Write-Host ""
