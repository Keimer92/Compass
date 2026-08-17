@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"

rem Ejecuta el bloque PowerShell embebido abajo (despues de REM PS_BEGIN)
powershell -NoProfile -ExecutionPolicy Bypass -Command "$s=Get-Content -LiteralPath '%~f0' -Raw -Encoding UTF8; $s=$s -replace '(?s)^.*REM PS_BEGIN','' -replace '(?s)REM PS_END.*$',''; iex $s"
set "EXITCODE=%errorlevel%"
echo.
pause
exit /b %EXITCODE%

REM PS_BEGIN
$root = (Get-Location).Path

$pkgFile  = 'package.json'
$cargoFile = 'src-tauri/Cargo.toml'
$tauriFile = 'src-tauri/tauri.conf.json'

$pkgText  = [IO.File]::ReadAllText((Join-Path $root $pkgFile))
$cargoText = [IO.File]::ReadAllText((Join-Path $root $cargoFile))
$tauriText = [IO.File]::ReadAllText((Join-Path $root $tauriFile))

$m = [regex]::Match($pkgText, '"version"\s*:\s*"([^"]+)"')
$pkgVer = if ($m.Success) { $m.Groups[1].Value } else { '' }
$m = [regex]::Match($cargoText, '(?m)^version\s*=\s*"([^"]+)"')
$cargoVer = if ($m.Success) { $m.Groups[1].Value } else { '' }
$m = [regex]::Match($tauriText, '"version"\s*:\s*"([^"]+)"')
$tauriVer = if ($m.Success) { $m.Groups[1].Value } else { '' }

if (-not $pkgVer -or -not $cargoVer -or -not $tauriVer) {
    Write-Host '  [ERROR] No se pudo leer la version de algun archivo.' -ForegroundColor Red
    exit 1
}

Write-Host ''
Write-Host '=========================================='
Write-Host '  VERSIONADOR TAURI'
Write-Host '=========================================='
Write-Host ''
Write-Host ('  package.json      : ' + $pkgVer)
Write-Host ('  Cargo.toml        : ' + $cargoVer)
Write-Host ('  tauri.conf.json   : ' + $tauriVer)
Write-Host ''
$new = Read-Host '  Nueva version (X.Y.Z, opcional sufijo -pre/+build)'
$new = $new.Trim()

if ($new -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
    Write-Host '  [ERROR] Formato invalido. Usa X.Y.Z (ej: 1.2.3) o con sufijo (ej: 2.0.0-beta.1)' -ForegroundColor Red
    exit 1
}

$pkgText2  = [regex]::Replace($pkgText, '"version"\s*:\s*"[^"]+"', ('"version": "' + $new + '"'))
[IO.File]::WriteAllText((Join-Path $root $pkgFile), $pkgText2)

$tauriText2 = [regex]::Replace($tauriText, '"version"\s*:\s*"[^"]+"', ('"version": "' + $new + '"'))
[IO.File]::WriteAllText((Join-Path $root $tauriFile), $tauriText2)

$cargoText2 = [regex]::Replace($cargoText, '(?m)^version\s*=\s*"[^"]+"', ('version = "' + $new + '"'))
[IO.File]::WriteAllText((Join-Path $root $cargoFile), $cargoText2)

Write-Host ('  [OK] Version actualizada a ' + $new + ' en los 3 archivos') -ForegroundColor Green
REM PS_END
