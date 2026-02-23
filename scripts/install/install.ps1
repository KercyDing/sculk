# Check execution policy
$executionPolicy = Get-ExecutionPolicy -Scope CurrentUser
if ($executionPolicy -eq "Restricted" -or $executionPolicy -eq "Undefined") {
    Write-Host "Error: PowerShell script execution is disabled on this system." -ForegroundColor Red
    Write-Host ""
    Write-Host "To allow script execution, run the following command:" -ForegroundColor Yellow
    Write-Host "  Set-ExecutionPolicy RemoteSigned -Scope CurrentUser" -ForegroundColor White
    Write-Host ""
    Write-Host "Then run this installation script again." -ForegroundColor Yellow
    exit 1
}

$ErrorActionPreference = "Stop"

Write-Host "Downloading sculk for Windows..." -ForegroundColor Green

$DOWNLOAD_URL = "https://github.com/SeaLantern-Studio/sculk/releases/latest/download/sculk-windows-amd64.exe"
$INSTALL_DIR = "$env:LOCALAPPDATA\sculk"
$INSTALL_PATH = "$INSTALL_DIR\sculk.exe"

# Create install directory
New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null

# Download binary
try {
    Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile $INSTALL_PATH -UseBasicParsing
} catch {
    Write-Host "Error: Failed to download sculk" -ForegroundColor Red
    exit 1
}

Write-Host "sculk installed to $INSTALL_PATH" -ForegroundColor Green

# Add to PATH if not already present
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$INSTALL_DIR*") {
    Write-Host "Adding $INSTALL_DIR to PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$INSTALL_DIR", "User")
    $env:Path = "$env:Path;$INSTALL_DIR"
    Write-Host "PATH updated." -ForegroundColor Green
}

# Verify installation
Write-Host ""
try {
    $version = & sculk --version
    Write-Host "Verification: $version" -ForegroundColor Green
} catch {
    Write-Host "Please restart your terminal to use sculk." -ForegroundColor Cyan
}
Write-Host ""
Write-Host "sculk installed successfully!" -ForegroundColor Green
Write-Host ""
