$ErrorActionPreference = "Stop"

Write-Host "Uninstalling sculk..." -ForegroundColor Green

# Remove install directory
$INSTALL_DIR = "$env:LOCALAPPDATA\sculk"
if (Test-Path $INSTALL_DIR) {
    Remove-Item -Path $INSTALL_DIR -Recurse -Force
    Write-Host "Removed: $INSTALL_DIR" -ForegroundColor Green
} else {
    Write-Host "Warning: sculk directory not found at $INSTALL_DIR" -ForegroundColor Yellow
}

# Remove from PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -like "*$INSTALL_DIR*") {
    Write-Host "Removing from PATH..." -ForegroundColor Cyan
    $pathArray = $userPath -split ';' | Where-Object { $_ -ne $INSTALL_DIR -and $_ -ne "" }
    $newPath = $pathArray -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "Removed from PATH" -ForegroundColor Green
}

Write-Host ""
Write-Host "sculk uninstalled successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "Please restart your terminal for changes to take effect." -ForegroundColor Cyan
Write-Host ""
