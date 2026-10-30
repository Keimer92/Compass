@echo off
setlocal EnableDelayedExpansion
title Instalando Compass...
echo.
echo =============================================
echo   Compass for Rust - Instalador
echo =============================================
echo.

REM ---------------------------------------------------------------
REM  Flags:  install.bat --force   rehace todo aunque no haga falta
REM ---------------------------------------------------------------
set "FORCE=0"
if /i "%~1"=="--force" set "FORCE=1"
if /i "%~1"=="-f"     set "FORCE=1"

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
echo raiz del proyecto ^(Compass^).
echo.
pause
exit /b 1
)
if not exist "assets\install-probe.ps1" (
echo [ERROR] No se encuentra assets\install-probe.ps1
echo No se puede decidir si hay que recompilar.
echo.
pause
exit /b 1
)

echo.
tasklist /fi "imagename eq compass.exe" 2>nul | find /i "compass.exe" >nul
if not errorlevel 1 (
    echo [INFO] Cerrando compass.exe en ejecucion...
    taskkill /f /im compass.exe >nul 2>nul
    if errorlevel 1 (
        echo [WARN] No se pudo cerrar compass.exe. Intentando continuar...
    ) else (
        echo [OK] compass.exe cerrado.
    )
    REM `timeout` protesta si el stdin esta redirigido (ejecucion desde script).
    REM Es una pausa de un segundo, no importa si se pierde.
    timeout /t 1 /nobreak >nul 2>nul
)

REM ---------------------------------------------------------------
REM  Estado del sistema, en una sola invocacion de PowerShell.
REM  Antes eran tres (dos sondas del registro) y ademas la de Vibe Coding
REM  miraba una clave que no existe, asi que se reinstalaba siempre.
REM
REM  Se prefiere pwsh (PowerShell 7): arranca mas rapido y es el shell que
REM  usan las ventanas de Compass. Se comprueba ejecutandolo, no con `where`:
REM  el alias de la Microsoft Store es un stub de 0 bytes que `where`
REM  encuentra pero no arranca.
REM ---------------------------------------------------------------
set "PSSHELL=powershell"
pwsh -NoLogo -NoProfile -Command "exit 0" >nul 2>nul
if not errorlevel 1 set "PSSHELL=pwsh"

set "STATE="
for /f "tokens=1,2,3,4 delims=;" %%a in ('%PSSHELL% -NoLogo -NoProfile -ExecutionPolicy Bypass -File "assets\install-probe.ps1"') do set "STATE=%%a;%%b;%%c;%%d"

if not defined STATE (
    echo [WARN] No se pudo leer el estado del sistema.
    echo [WARN] Se asume que hay que reinstallarlo todo.
    set "STATE=1;1;1;4"
)

set "NEED_BUILD="
set "NEED_CR="
set "NEED_VIBE="
set "MISSING_TOOLS="
for /f "tokens=1,2,3,4 delims=;" %%a in ("%STATE%") do (
    set "NEED_BUILD=%%a"
    set "NEED_CR=%%b"
    set "NEED_VIBE=%%c"
    set "MISSING_TOOLS=%%d"
)
if "%FORCE%"=="1" (
    set "NEED_BUILD=1"
    set "NEED_CR=1"
    set "NEED_VIBE=1"
    set "MISSING_TOOLS=4"
    echo [INFO] Modo --force: se rehace todo aunque ya este al dia.
    echo.
)

REM La tarea vibe-setup pregunta por las herramientas que falten, asi que hace
REM falta aunque el menu ya este instalado: si el usuario acaba de instalar
REM Cline a mano, su sub-entrada todavia no existe en el menu.
set "NEED_VIBE_SETUP=%NEED_VIBE%"
if not "%MISSING_TOOLS%"=="0" set "NEED_VIBE_SETUP=1"

REM ---------------------------------------------------------------
REM  1. Compilar e instalar
REM ---------------------------------------------------------------
set "EXIT_CODE=0"
if "%NEED_BUILD%"=="1" (
    echo [INFO] Compilando e instalando...
    cargo install --path . --force
    REM Delayed expansion: sin `!errorlevel!` se evaluaria al parsear el
    REM bloque, antes de que cargo haya corrido, y siempre daria 0.
    if !errorlevel! neq 0 set "EXIT_CODE=1"
    echo.
    echo [INFO] Compilacion finalizada.
) else (
    echo [SKIP] El binario instalado ya esta al dia ^(no hay fuentes mas nuevas^).
    echo [INFO] Usa install.bat --force si quieres forzar la recompilacion.
)

if "%EXIT_CODE%"=="1" (
    echo.
    echo =============================================
    echo   [WARN] La compilacion tuvo problemas.
    echo =============================================
    echo.
    echo Se usara la version anterior de compass si esta instalada.
    echo Se omite la configuracion de menus para no apuntar a un binario roto.
    set "NEED_CR=0"
    set "NEED_VIBE=0"
    set "NEED_VIBE_SETUP=0"
    echo.
)

echo.
echo [INFO] Copiando iconos al directorio del binario...

:: icon.ico (Compass personalizado) -> Default.ico
if exist "assets\icon\icon.ico" (
    copy /Y "assets\icon\icon.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] icon.ico copiado
) else if exist "assets\icon\Default.ico" (
    copy /Y "assets\icon\Default.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] Default.ico copiado
)

:: opencode.ico (Vibe Coding: OpenCode/Cline/Kilo) -> Default.ico
if exist "assets\icon\opencode.ico" (
    copy /Y "assets\icon\opencode.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] opencode.ico copiado
) else if exist "assets\icon\Default.ico" (
    copy /Y "assets\icon\Default.ico" "%USERPROFILE%\.cargo\bin\" >nul 2>nul
    echo [OK] Default.ico copiado ^(para Vibe Coding^)
)
echo.

echo [INFO] Configurando menu contextual de Compass...
if "%NEED_CR%"=="1" (
    call "%USERPROFILE%\.cargo\bin\compass.exe" --task context-menu-install
    if errorlevel 1 (
        echo [WARN] No se pudo instalar el menu contextual de Compass.
    ) else (
        echo [OK] Menu contextual de Compass instalado.
    )
) else (
    echo [SKIP] Menu contextual de Compass ya instalado.
)
echo.

echo [INFO] Configurando menu contextual de Vibe Coding...
if "%NEED_VIBE_SETUP%"=="1" (
    if not "%MISSING_TOOLS%"=="0" (
        echo [INFO] ^(Se preguntara por las herramientas de Vibe Coding que falten^)
    )
    call "%USERPROFILE%\.cargo\bin\compass.exe" --task vibe-setup
    if errorlevel 1 (
        echo [WARN] No se pudo instalar el menu contextual de Vibe Coding.
    ) else (
        echo [OK] Menu contextual de Vibe Coding instalado.
    )
) else (
    echo [SKIP] Menu contextual de Vibe Coding ya instalado y las herramientas presentes.
)

echo.
if "%EXIT_CODE%"=="1" exit /b 1

echo =============================================
echo   [OK] Instalacion completada
echo =============================================
echo.
echo Si el menu no aparece, reinicia el Explorador:
echo   taskkill /f /im explorer.exe ^& start explorer.exe
echo.
exit /b 0