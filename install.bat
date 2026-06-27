@echo off
title Instalando Code Runner...
echo.
echo =============================================
echo   Code Runner for Rust / Tauri - Instalador
echo =============================================
echo.
where cargo >nul 2>nul
if %errorlevel% neq 0 (
echo [ERROR] Cargo - Rust no esta instalado.
echo.
echo Para instalarlo:
echo   1. Ve a https://rustup.rs/
echo   2. Descarga y ejecuta rustup-init.exe
echo   3. Vuelve a ejecutar este instalador.
echo.
pause
exit /b 1
)
if not exist "Cargo.toml" (
echo [ERROR] No se encuentra Cargo.toml.
echo Asegurate de que install.bat este en la carpeta
echo raiz del proyecto ^(Code Runner^).
echo.
pause
exit /b 1
)
echo.
tasklist /fi "imagename eq code-runner.exe" 2>nul | find /i "code-runner.exe" >nul
if not errorlevel 1 (
    echo [INFO] Cerrando code-runner.exe en ejecucion...
    taskkill /f /im code-runner.exe >nul 2>nul
    if errorlevel 1 (
        echo [WARN] No se pudo cerrar code-runner.exe. Intentando continuar...
    ) else (
        echo [OK] code-runner.exe cerrado.
    )
    timeout /t 1 /nobreak >nul
)
echo.
echo [INFO] Compilando e instalando...
cargo install --path . --force
set "EXIT_CODE=%errorlevel%"
echo.
echo Presiona cualquier tecla para ver el resultado...
pause >nul
if "%EXIT_CODE%"=="0" (
echo.
echo =============================================
echo     [OK] Instalacion completada!
echo =============================================
echo.
echo Ya puedes usar el comando 'code-runner'
echo desde cualquier terminal.
echo.
) else (
echo.
echo [ERROR] La instalacion fallo. Revisa los errores arriba.
echo.
)
pause
exit /b %EXIT_CODE%
