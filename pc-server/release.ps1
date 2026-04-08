$ErrorActionPreference = "Stop"

Write-Host "Building release..." -ForegroundColor Green
cargo build --release
if ($LASTEXITCODE -ne 0) { exit 1 }

$releaseDir = "release"
if (Test-Path $releaseDir) { 
    Get-Process -Name "usb-sms-reader" -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Remove-Item $releaseDir -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $releaseDir) {
        Write-Host "Warning: could not clean release dir, some files may be locked" -ForegroundColor Yellow
    }
}
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null

Write-Host "Copying files..." -ForegroundColor Green
Copy-Item "target\release\usb-sms-reader.exe" $releaseDir
Copy-Item "config\config.yaml" $releaseDir

if (Test-Path "tools") {
    New-Item -ItemType Directory -Path "$releaseDir\tools" -Force | Out-Null
    Copy-Item "tools\*" "$releaseDir\tools\" -Recurse
}

Write-Host "Release package ready in .\release\" -ForegroundColor Green
Get-ChildItem $releaseDir -Recurse | ForEach-Object { Write-Host "  $($_.FullName.Replace((Get-Location).Path + '\', ''))" }
