@echo off
title Instalar Menu Contextual - Code Runner
chcp 65001 >nul
echo.
echo =============================================
echo   Instalar Menu Contextual - Code Runner
echo =============================================
echo.

:: Buscar code-runner en PATH o en la carpeta de Cargo
where code-runner >nul 2>nul
if %errorlevel% equ 0 (
    set "CR=code-runner"
) else (
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
