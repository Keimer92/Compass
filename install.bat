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
echo [INFO] Compilacion finalizada.

if "%EXIT_CODE%"=="0" (
echo.
echo =============================================
echo     [OK] Instalacion completada!
echo =============================================
echo.
echo Ya puedes usar el comando 'code-runner'
echo desde cualquier terminal.
) else (
echo.
echo =============================================
echo   [WARN] La compilacion tuvo problemas.
echo =============================================
echo.
echo Se usara la version anterior de code-runner
echo si esta instalada.
)
echo.

echo [INFO] Copiando iconos al directorio del binario...

:: icon.ico (Code Runner personalizado) -> Default.ico
if exist "assets\icon\icon.ico" (
    copy /Y "assets\icon\icon.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] icon.ico copiado
) else if exist "assets\icon\Default.ico" (
    copy /Y "assets\icon\Default.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] Default.ico copiado
)

:: opencode.ico (OpenCode personalizado) -> Default.ico
if exist "assets\icon\opencode.ico" (
    copy /Y "assets\icon\opencode.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] opencode.ico copiado
) else if exist "assets\icon\Default.ico" (
    copy /Y "assets\icon\Default.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] Default.ico copiado (para OpenCode)
)
echo.

echo [INFO] Configurando menu contextual de Code Runner...
for /f %%i in ('powershell -NoProfile -Command "$v=(Get-ItemProperty -Path 'HKCU:\Software\Classes\Directory\shell\zz_CodeRunner\command' -ErrorAction SilentlyContinue).'(default)'; if($v -like '*code-runner.exe*'){'1'}else{'0'}"') do set "CR_CURRENT=%%i"
if "%CR_CURRENT%"=="1" (
    echo [SKIP] Menu contextual de Code Runner ya instalado.
) else (
    call "%USERPROFILE%\.cargo\bin\code-runner.exe" --task context-menu-install
    if errorlevel 1 (
        echo [WARN] No se pudo instalar el menu contextual de Code Runner.
    ) else (
        echo [OK] Menu contextual de Code Runner instalado.
    )
)
echo.

echo [INFO] Configurando menu contextual de OpenCode...
for /f %%i in ('powershell -NoProfile -Command "$v=(Get-ItemProperty -Path 'HKCU:\Software\Classes\Directory\shell\zz_OpenCode\command' -ErrorAction SilentlyContinue).'(default)'; if($v -like '*opencode*'){'1'}else{'0'}"') do set "OC_CURRENT=%%i"
if "%OC_CURRENT%"=="1" (
    echo [SKIP] Menu contextual de OpenCode ya instalado.
) else (
    call "%USERPROFILE%\.cargo\bin\code-runner.exe" --task opencode-install
    if errorlevel 1 (
        echo [WARN] No se pudo instalar el menu contextual de OpenCode.
    ) else (
        echo [OK] Menu contextual de OpenCode instalado.
    )
)

echo [INFO] Configurando menu contextual de Cline...
for /f %%i in ('powershell -NoProfile -Command "$v=(Get-ItemProperty -Path 'HKCU:\Software\Classes\Directory\shell\zz_pCline\command' -ErrorAction SilentlyContinue).'(default)'; if($v -like '*cline*'){'1'}else{'0'}"') do set "CL_CURRENT=%%i"
if "%CL_CURRENT%"=="1" (
    echo [SKIP] Menu contextual de Cline ya instalado.
) else (
    call "%USERPROFILE%\.cargo\bin\code-runner.exe" --task cline-install
    if errorlevel 1 (
        echo [WARN] No se pudo instalar el menu contextual de Cline.
    ) else (
        echo [OK] Menu contextual de Cline instalado.
    )
)
echo.

echo [INFO] Configurando menu contextual de Kilo Code...
for /f %%i in ('powershell -NoProfile -Command "$v=(Get-ItemProperty -Path 'HKCU:\Software\Classes\Directory\shell\zz_pKilo\command' -ErrorAction SilentlyContinue).'(default)'; if($v -like '*kilo*'){'1'}else{'0'}"') do set "KL_CURRENT=%%i"
if "%KL_CURRENT%"=="1" (
    echo [SKIP] Menu contextual de Kilo Code ya instalado.
) else (
    call "%USERPROFILE%\.cargo\bin\code-runner.exe" --task kilo-install
    if errorlevel 1 (
        echo [WARN] No se pudo instalar el menu contextual de Kilo Code.
    ) else (
        echo [OK] Menu contextual de Kilo Code instalado.
    )
)

timeout /t 3 /nobreak >nul
exit /b %EXIT_CODE%
