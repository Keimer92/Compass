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

$pkgFile    = Join-Path $root 'package.json'
$cargoTauri = Join-Path $root 'src-tauri\Cargo.toml'
$tauriCfg   = Join-Path $root 'src-tauri\tauri.conf.json'
$cargoRoot  = Join-Path $root 'Cargo.toml'

$isTauri = Test-Path -LiteralPath $tauriCfg
$isRust  = Test-Path -LiteralPath $cargoRoot

if (-not $isTauri -and -not $isRust) {
    Write-Host '  [ERROR] No se detecto proyecto Tauri (src-tauri\tauri.conf.json) ni Rust (Cargo.toml).' -ForegroundColor Red
    exit 1
}

# Archivos de version a actualizar segun el tipo de proyecto
$versionFiles = @()
if ($isTauri) {
    $versionFiles += @(
        @{ Path = $pkgFile;    Kind = 'json' },
        @{ Path = $tauriCfg;   Kind = 'json' },
        @{ Path = $cargoTauri; Kind = 'toml' }
    )
} else {
    $versionFiles += @(
        @{ Path = $cargoRoot; Kind = 'toml' }
    )
}

# Leer versiones actuales de cada archivo.
# Fase de VALIDACION: si algo falla aqui, abortamos sin escribir nada (todo-o-nada).
$files = @()
foreach ($vf in $versionFiles) {
    if (-not (Test-Path -LiteralPath $vf.Path)) {
        Write-Host ('  [ERROR] No existe el archivo: ' + $vf.Path) -ForegroundColor Red
        exit 1
    }
    $text = [IO.File]::ReadAllText($vf.Path)
    $regex = if ($vf.Kind -eq 'json') { '"version"\s*:\s*"([^"]+)"' } else { '(?m)^version\s*=\s*"([^"]+)"' }
    $m = [regex]::Match($text, $regex)

    if ($m.Success) {
        $files += @{ Path = $vf.Path; Kind = $vf.Kind; Current = $m.Groups[1].Value; Text = $text }
        continue
    }

    # Sin campo version: comprobar que se puede INSERTAR de forma segura.
    if ($vf.Kind -eq 'toml') {
        if (-not [regex]::IsMatch($text, '(?m)^\[package\]\s*$')) {
            Write-Host ('  [ERROR] Cargo.toml sin version y sin seccion [package]: no se puede insertar de forma segura.' + "`n" + '          Archivo: ' + $vf.Path) -ForegroundColor Red
            exit 1
        }
        $files += @{ Path = $vf.Path; Kind = $vf.Kind; Current = $null; Text = $text }
    } else {
        # JSON: validar que es JSON valido y un objeto (si no, no tocar).
        $obj = $null
        try { $obj = $text | ConvertFrom-Json } catch { $obj = $null }
        if ($null -eq $obj -or $obj -isnot [System.Management.Automation.PSCustomObject]) {
            Write-Host ('  [ERROR] JSON invalido o no es un objeto: no se puede insertar version de forma segura.' + "`n" + '          Archivo: ' + $vf.Path) -ForegroundColor Red
            exit 1
        }
        if ($text.IndexOf('{') -lt 0) {
            Write-Host ('  [ERROR] No se encontro el inicio del objeto JSON: ' + $vf.Path) -ForegroundColor Red
            exit 1
        }
        $files += @{ Path = $vf.Path; Kind = $vf.Kind; Current = $null; Text = $text }
    }
}

Write-Host ''
Write-Host '=========================================='
if ($isTauri) { Write-Host '  VERSIONADOR TAURI' } else { Write-Host '  VERSIONADOR RUST' }
Write-Host '=========================================='
Write-Host ''
foreach ($f in $files) {
    $rel = $f.Path.Substring($root.Length + 1)
    $cur = if ($null -eq $f.Current) { '(sin version - se agregara)' } else { $f.Current }
    Write-Host ('  {0,-32} : {1}' -f $rel, $cur)
}
Write-Host ''
$new = Read-Host '  Nueva version (X.Y.Z, opcional sufijo -pre/+build)'
if ($null -eq $new) { $new = '' }
$new = $new.Trim()

if ($new -eq '') {
    Write-Host '  [ERROR] Version vacia.' -ForegroundColor Red
    exit 1
}

if ($new -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
    Write-Host '  [ERROR] Formato invalido. Usa X.Y.Z (ej: 1.2.3) o con sufijo (ej: 2.0.0-beta.1)' -ForegroundColor Red
    exit 1
}

# Escribir la nueva version: reemplaza donde existe, INSERTA donde falta.
foreach ($f in $files) {
    if ($null -ne $f.Current) {
        if ($f.Kind -eq 'json') {
            $f.Text = [regex]::Replace($f.Text, '"version"\s*:\s*"[^"]+"', ('"version": "' + $new + '"'))
        } else {
            $f.Text = [regex]::Replace($f.Text, '(?m)^version\s*=\s*"[^"]+"', ('version = "' + $new + '"'))
        }
    } else {
        if ($f.Kind -eq 'toml') {
            $insert = '[package]' + "`n" + 'version = "' + $new + '"'
            $f.Text = [regex]::Replace($f.Text, '(?m)^\[package\]\s*$', $insert, 1)
        } else {
            $idx = $f.Text.IndexOf('{')
            $prop = '"version": "' + $new + '"'
            if ($f.Text.Substring($idx + 1).TrimStart().StartsWith('}')) {
                $f.Text = $f.Text.Substring(0, $idx + 1) + $prop + $f.Text.Substring($idx + 1)
            } else {
                $f.Text = $f.Text.Substring(0, $idx + 1) + $prop + ', ' + $f.Text.Substring($idx + 1)
            }
        }
    }
    [IO.File]::WriteAllText($f.Path, $f.Text)
}

$added = @($files | Where-Object { $null -eq $_.Current }).Count
Write-Host ('  [OK] Version actualizada a ' + $new + ' en ' + $files.Count + ' archivo(s)') -ForegroundColor Green
if ($added -gt 0) {
    Write-Host ('  [OK] Se inserto el campo version en ' + $added + ' archivo(s) que no lo tenian') -ForegroundColor Green
}
REM PS_END