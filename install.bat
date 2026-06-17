@echo off
chcp 65001 >nul
title Instalando Developing Runner...

:: ─────────────────────────────────────────────
::  Developing Runner — Instalador Rápido
::  Ejecuta este .bat para instalar o actualizar
::  el comando 'developing-runner' globalmente.
:: ─────────────────────────────────────────────

echo.
echo  ╔════════════════════════════════════════════╗
echo  ║     Developing Runner — Instalador         ║
echo  ╚════════════════════════════════════════════╝
echo.

:: 1. Verificar que Cargo (Rust) esté instalado
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo  [ERROR] Cargo (Rust) no está instalado.
    echo.
    echo  Para instalarlo:
    echo    1. Ve a https://rustup.rs/
    echo    2. Descarga y ejecuta rustup-init.exe
    echo    3. Vuelve a ejecutar este instalador.
    echo.
    pause
    exit /b 1
)

:: 2. Verificar que estamos en la carpeta correcta
if not exist "Cargo.toml" (
    echo  [ERROR] No se encuentra Cargo.toml.
    echo  Asegúrate de que install.bat esté en la carpeta
    echo  raíz del proyecto (Developing Runner).
    echo.
    pause
    exit /b 1
)

:: 3. Preguntar si ya está instalado
echo  El comando 'developing-runner' será instalado globalmente.
echo.

set /p CONFIRM="  Presiona S para instalar/actualizar, o N para cancelar (s/n): "
if /i not "%CONFIRM%"=="s" (
    echo.
    echo  [INFO] Instalacion cancelada.
    pause
    exit /b 0
)

echo.
echo  [INFO] Compilando e instalando...

:: 4. Ejecutar la instalación
cargo install --path . --force

if %errorlevel% equ 0 (
    echo.
    echo  ╔════════════════════════════════════════════╗
    echo  ║     [OK] Instalacion completada!           ║
    echo  ╚════════════════════════════════════════════╝
    echo.
    echo  Ya puedes usar el comando 'developing-runner'
    echo  desde cualquier terminal.
    echo.
) else (
    echo.
    echo  [ERROR] La instalacion fallo. Revisa los errores arriba.
    echo.
)

pause
exit /b %errorlevel%
