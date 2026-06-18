@echo off
set "RUNNER=%USERPROFILE%\.cargo\bin\developing-runner.exe"
if not exist "%RUNNER%" (
    echo [ERROR] No se encontro developing-runner.exe
    echo Ejecuta: cargo install --path "%~dp0"
    pause
    exit /b 1
)
"%RUNNER%" -p "%CD%"
pause