@echo off
set "RUNNER=%USERPROFILE%\.cargo\bin\code-runner.exe"
if not exist "%RUNNER%" (
    echo [ERROR] No se encontro code-runner.exe
    echo Ejecuta: cargo install --path "%~dp0"
    pause
    exit /b 1
)
"%RUNNER%" -p "%CD%"
pause
