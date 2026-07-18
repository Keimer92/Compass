param(
  [string]$Target = ""
)

$ErrorActionPreference = "Stop"

$LogFile = "$PSScriptRoot\build_error.log"
$ProjectRoot = "D:\Mis Proyectos\Flutter\School Tracker Pro ELA"

function Show-Menu {
  Clear-Host
  Write-Host "========================================" -ForegroundColor Cyan
  Write-Host "  School Tracker Pro ELA - Build/Run" -ForegroundColor Cyan
  Write-Host "========================================" -ForegroundColor Cyan
  Write-Host ""
  Write-Host "  1) Ejecutar Windows (desarrollo)" -ForegroundColor Yellow
  Write-Host "  2) Ejecutar Android (desarrollo)" -ForegroundColor Yellow
  Write-Host "  3) Compilar Windows (release)" -ForegroundColor Yellow
  Write-Host "  4) Compilar Android (release APK)" -ForegroundColor Yellow
  Write-Host "  5) Compilar Android (release AAB)" -ForegroundColor Yellow
  Write-Host ""
  Write-Host "  Q) Salir" -ForegroundColor Red
  Write-Host ""
}

function Test-Flutter {
  if (-not (Get-Command flutter -ErrorAction SilentlyContinue)) {
    Write-Host "[ERROR] Flutter no encontrado en el PATH." -ForegroundColor Red
    pause; exit 1
  }
  if (-not (Test-Path "$ProjectRoot\pubspec.yaml")) {
    Write-Host "[ERROR] No se encuentra pubspec.yaml en $ProjectRoot" -ForegroundColor Red
    pause; exit 1
  }
  Set-Location $ProjectRoot
}

function Invoke-Run {
  param([string]$Command, [string]$RunName)

  $script = @"
`$ErrorActionPreference = 'Stop'
Write-Host '========================================' -ForegroundColor Cyan
Write-Host '  School Tracker Pro ELA - $RunName' -ForegroundColor Cyan
Write-Host '========================================' -ForegroundColor Cyan
Write-Host ''
Set-Location '$ProjectRoot'
Write-Host '  >> flutter pub get' -ForegroundColor Gray
flutter pub get 2>&1 | Out-Null
if (`$LASTEXITCODE -ne 0) { Read-Host 'Enter para cerrar'; exit }
"@

  if ($RunName -like "*Windows*") {
    $script += @"

Write-Host '  >> npm install (WhatsApp engine)' -ForegroundColor Gray
Push-Location whatsapp_engine
`$originalErrorAction = `$ErrorActionPreference
`$ErrorActionPreference = 'Continue'
npm install 2>`$null | Out-Null
`$ErrorActionPreference = `$originalErrorAction
Pop-Location
"@
  }

  $script += @"

Write-Host '  >> $Command' -ForegroundColor Gray
Write-Host ''
Invoke-Expression '$Command'
Write-Host ''
Write-Host '[FIN] La aplicacion se cerro.' -ForegroundColor Cyan
Read-Host 'Enter para cerrar'
"@

  Start-Process powershell -ArgumentList "-NoExit", "-Command", $script -WindowStyle Normal
}

function Invoke-Build {
  param([string]$Command, [string]$OutputLabel, [string]$BuildName)

  $script = @"
`$ErrorActionPreference = 'Stop'
`$LogFile = '$PSScriptRoot\build_error.log'
Write-Host '========================================' -ForegroundColor Cyan
Write-Host '  School Tracker Pro ELA - $BuildName' -ForegroundColor Cyan
Write-Host '========================================' -ForegroundColor Cyan
Write-Host ''
Set-Location '$ProjectRoot'
Write-Host '  >> flutter pub get' -ForegroundColor Gray
flutter pub get 2>&1 | Out-Null
if (`$LASTEXITCODE -ne 0) { Read-Host 'Enter para cerrar'; exit }
"@

  if ($BuildName -like "*Windows*") {
    $script += @"

Write-Host '  >> npm install (WhatsApp engine)' -ForegroundColor Gray
Push-Location whatsapp_engine
`$originalErrorAction = `$ErrorActionPreference
`$ErrorActionPreference = 'Continue'
npm install 2>`$null | Out-Null
`$ErrorActionPreference = `$originalErrorAction
Pop-Location
"@
  }

  $script += @"

Write-Host '  >> $Command' -ForegroundColor Gray
Invoke-Expression '$Command' 2> `$LogFile
`$exitCode = `$LASTEXITCODE
Write-Host ''
if (`$exitCode -eq 0) {
  Write-Host '=========== BUILD EXITOSO ===========' -ForegroundColor Green
  Write-Host '  Compilacion: $BuildName' -ForegroundColor Green
  Write-Host '  Salida:      $OutputLabel' -ForegroundColor Green
  if (Test-Path `$LogFile) { Remove-Item `$LogFile -Force }
} else {
  Write-Host '=========== BUILD FALLIDO ===========' -ForegroundColor Red
  Write-Host '  Codigo:      `$exitCode' -ForegroundColor Red
  Write-Host ''
  if (Test-Path `$LogFile) {
    Write-Host '--- ERROR ---' -ForegroundColor Red
    Get-Content `$LogFile | ForEach-Object { Write-Host "  `$_" -ForegroundColor Red }
  }
  Write-Host '[LOG] Error guardado en: `$LogFile' -ForegroundColor Yellow
}
Read-Host 'Enter para cerrar'
"@

  Start-Process powershell -ArgumentList "-NoExit", "-Command", $script -WindowStyle Normal
}

function Run-Windows      { Invoke-Run -Command "flutter run -d windows" -RunName "Windows (desarrollo)" }
function Run-Android      { Invoke-Run -Command "flutter run" -RunName "Android (desarrollo)" }
function Build-WinRelease  { Invoke-Build -Command "flutter build windows --release" -OutputLabel "build\windows\runner\Release\" -BuildName "Windows Release" }
function Build-AndRelease  { Invoke-Build -Command "flutter build apk --release" -OutputLabel "build\app\outputs\flutter-apk\app-release.apk" -BuildName "Android Release APK" }
function Build-AndBundle   { Invoke-Build -Command "flutter build appbundle --release" -OutputLabel "build\app\outputs\bundle\release\app-release.aab" -BuildName "Android Release AAB" }

# ── Direct mode ──
if ($Target) {
  Test-Flutter
  switch ($Target.ToLower()) {
    "run-windows"     { Run-Windows; return }
    "run-android"     { Run-Android; return }
    "windows-release" { Build-WinRelease; return }
    "android-release" { Build-AndRelease; return }
    "android-bundle"  { Build-AndBundle; return }
    default { Write-Host "Targets: run-windows, run-android, windows-release, android-release, android-bundle" -ForegroundColor Red }
  }
  return
}

# ── Interactive mode ──
Test-Flutter
do {
  Show-Menu
  $choice = Read-Host "Opcion"
  switch ($choice) {
    "1" { Run-Windows }
    "2" { Run-Android }
    "3" { Build-WinRelease }
    "4" { Build-AndRelease }
    "5" { Build-AndBundle }
    "q" { Write-Host "Adios!" -ForegroundColor Cyan; break }
    "Q" { Write-Host "Adios!" -ForegroundColor Cyan; break }
  }
} while ($choice -notin @("q", "Q"))
