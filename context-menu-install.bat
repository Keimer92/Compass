@echo off
title Instalar Menu Contextual - Code Runner & OpenCode
chcp 65001 >nul
echo.
echo =============================================
echo   Instalar Menu Contextual
echo   Code Runner + OpenCode
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

:: Copiar iconos junto al binario si existen localmente
for %%I in ("%CR%") do set "BIN_DIR=%%~dpI"
if not defined BIN_DIR set "BIN_DIR=%USERPROFILE%\.cargo\bin\"

echo [INFO] Copiando iconos al directorio del binario...

:: icon.ico (Code Runner personalizado) -> Default.ico (fallback)
if exist "assets\icon\icon.ico" (
    copy /Y "assets\icon\icon.ico" "%BIN_DIR%" >nul 2>nul
    echo [OK] icon.ico copiado
) else if exist "assets\icon\Default.ico" (
    copy /Y "assets\icon\Default.ico" "%BIN_DIR%" >nul 2>nul
    echo [OK] Default.ico copiado
)

:: opencode.ico (OpenCode personalizado) -> Default.ico (fallback)
if exist "assets\icon\opencode.ico" (
    copy /Y "assets\icon\opencode.ico" "%BIN_DIR%" >nul 2>nul
    echo [OK] opencode.ico copiado
) else if exist "assets\icon\Default.ico" (
    copy /Y "assets\icon\Default.ico" "%BIN_DIR%" >nul 2>nul
    echo [OK] Default.ico copiado
)
echo.

:: Instalar Code Runner
echo [1/2] Instalando menu contextual de Code Runner...
"%CR%" --task context-menu-install
if %errorlevel% neq 0 (
    echo [ERROR] Fallo Code Runner.
    pause
    exit /b 1
)
echo.

:: Instalar OpenCode
echo [2/2] Instalando menu contextual de OpenCode...
"%CR%" --task opencode-install
if %errorlevel% neq 0 (
    echo [WARN] Fallo OpenCode.
)
echo.

echo =============================================
echo     [OK] Menus contextuales instalados!
echo =============================================
echo.
echo  - Haz clic derecho en una carpeta
echo    → "Abrir con Code Runner"
echo    → "Open with OpenCode"
echo.
echo Para cambiar iconos, coloca estos archivos
echo en assets\icon\ y vuelve a ejecutar:
echo   icon.ico     → icono de Code Runner
echo   opencode.ico → icono de OpenCode
echo.
pause
