@echo off
echo start
if not exist "Cargo.toml" (
echo [ERROR] No Cargo.toml
pause
exit /b 1
)
echo El comando 'code-runner' sera instalado globalmente.
echo.
set /p "CONFIRM=Presiona S para instalar/actualizar, o N para cancelar (s/n): "
if /i not "%CONFIRM%"=="s" (
echo.
echo [INFO] Instalacion cancelada.
pause
exit /b 0
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
