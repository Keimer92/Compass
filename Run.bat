@echo off
chcp 65001 >nul
set "RUNNER=%USERPROFILE%\.cargo\bin\developing-runner.exe"
if not exist "%RUNNER%" (
    echo [ERROR] No se encontro developing-runner.exe
    echo Ejecuta: cargo install --path "%~dp0"
    pause
    exit /b 1
)
:: Ejecuta el CLI desde el directorio actual (se autodetecta el proyecto)
"%RUNNER%" -p "%CD%"
pause
