@echo off
title Instalar Menu Contextual - Code Runner
chcp 65001 >nul
echo.
echo =============================================
echo   Instalar Menu Contextual - Code Runner
echo =============================================
echo.

:: Buscar code-runner en PATH o en la carpeta de Cargo
for /f "delims=" %%F in ('where code-runner 2^>nul') do set "CR=%%~F"
if not defined CR (
    if exist "%USERPROFILE%\.cargo\bin\code-runner.exe" (
        set "CR=%USERPROFILE%\.cargo\bin\code-runner.exe"
    ) else (
        echo [ERROR] No se encuentra code-runner.exe
        echo.
        echo Asegurate de haber instalado Code Runner primero:
        echo   cargo install --path .
        echo.
        echo O ejecuta install.bat desde la carpeta del proyecto.
        echo.
        pause
        exit /b 1
    )
)

:: Copiar icono junto al binario si existe localmente
if exist "assets\icon\icons8-terminal-64.ico" (
    echo [INFO] Copiando icono al directorio del binario...
    for %%I in ("%CR%") do set "BIN_DIR=%%~dpI"
    if not defined BIN_DIR set "BIN_DIR=%USERPROFILE%\.cargo\bin\"
    copy /Y "assets\icon\icons8-terminal-64.ico" "%BIN_DIR%" >nul 2>nul
    if errorlevel 1 (
        echo [WARN] No se pudo copiar el icono.
    ) else (
        echo [OK] Icono copiado.
    )
    echo.
)

echo [INFO] Ejecutando: "%CR%" --task context-menu-install
echo.
"%CR%" --task context-menu-install
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Fallo la instalacion del menu contextual.
    pause
    exit /b 1
)

echo.
echo =============================================
echo     [OK] Menu contextual instalado!
echo =============================================
echo.
echo Ya puedes hacer clic derecho en cualquier
echo carpeta y seleccionar "Abrir con Code Runner".
echo.
pause
