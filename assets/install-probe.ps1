<#
    Sonda de estado para install.bat.

    Devuelve en una única línea, separada por punto y coma:

        <compilar>;<menu CR>;<menu vibe>;<herramientas que faltan>

    con 1 = hay que actuar y 0 = ya está al día. El cuarto campo es un contador y
    no una lista: lo usa `install.bat` sólo para decidir si hay que lanzar la
    tarea que pregunta por las que faltan, y el detalle lo dice esa tarea.

    Va en un fichero y no en un one-liner dentro de `for /f` porque las tuberías
    (`|`) son caracteres especiales de `cmd`: rompen el comando del bucle. Y
    además cada arranque de PowerShell cuesta (5.1 ~2,5 s, pwsh ~0,5 s), así que
    conviene una sola invocación para todas las comprobaciones en vez de varias.

    Ante cualquier error devuelve "1;1;1;4": es decir, "reinstálalo todo", que
    es lo seguro. Saltarse una instalación que sí hacía falta deja el sistema
    incoherente; Installerla de más sólo cuesta unos segundos.
#>

$ErrorActionPreference = 'SilentlyContinue'

$necesitaBuild = 1
$exe = Join-Path $env:USERPROFILE '.cargo\bin\compass.exe'

if (Test-Path $exe) {
    # El mtime del binario instalado contra el del archivo más reciente de las
    # fuentes. Heurística, y por eso existe `install.bat --force`: si tocas algo
    # que no está en esta lista (un build.rs, un assets que se incruste...), el
    # salto se come el cambio.
    $exeTime = (Get-Item $exe).LastWriteTime
    $nuevo = Get-ChildItem -Path src, Cargo.toml, Cargo.lock -Recurse -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

    if ($nuevo -and $nuevo.LastWriteTime -le $exeTime) {
        $necesitaBuild = 0
    }
}

# El menú de Compass es una entrada simple: `zz_Compass` con el path del
# ejecutable en `(default)`. Si no está, o apunta a otro sitio, hay que rehacerlo.
$menuCompass = 1
$comandoCr = (Get-ItemProperty `
    -Path 'HKCU:\Software\Classes\Directory\shell\zz_Compass\command').'(default)'
if ($comandoCr -like '*compass.exe*') {
    $menuCompass = 0
}
# Migración: antes se lanzaba `compass.exe` directo y la ventana mostraba el
# icono genérico de consola. Ahora la TUI también se aloja en PowerShell, así
# que una entrada sin `pwsh`/`powershell` se rehace.
if ($comandoCr -and $comandoCr -notlike '*pwsh*' -and $comandoCr -notlike '*powershell*') {
    $menuCompass = 1
}

# La acción .gitignore se agregó después de la instalación del menú base.
# También debe activar la reinstalación para migrar instalaciones existentes.
if ($menuCompass -eq 0 -and (
        -not (Test-Path 'HKCU:\Software\Classes\Directory\shell\zz_CompassGitignore\command') -or
        -not (Test-Path 'HKCU:\Software\Classes\Directory\Background\shell\zz_CompassGitignore\command')
    )) {
    $menuCompass = 1
}
$gitignoreFolderCommand = (Get-ItemProperty `
    -Path 'HKCU:\Software\Classes\Directory\shell\zz_CompassGitignore\command').'(default)'
$gitignoreBackgroundCommand = (Get-ItemProperty `
    -Path 'HKCU:\Software\Classes\Directory\Background\shell\zz_CompassGitignore\command').'(default)'
# `cmd.exe` es el marcador de una instalación anterior a la migración a
# PowerShell. Comparar el valor entero contra el nuevo sería frágil (cambia de
# ruta con cada versión de pwsh); el shell sí es estable.
if ($gitignoreFolderCommand -like '*cmd.exe*' -or
    $gitignoreBackgroundCommand -like '*cmd.exe*') {
    $menuCompass = 1
}

# El de Vibe Coding es cascada: la entrada padre `zz_VibeCoding` sólo lleva
# `ExtendedSubCommandsKey`, y las sub-entadas viven en otra ruta, con prefijo
# numérico (`01OpenCode`, `04Antigravity`...) porque Windows las ordena
# alfabéticamente. Hay que mirar ahí, no en `zz_VibeCoding\shell`.
#
# Además de existir, el menú tiene que haberlo instalado *este* binario: cada
# instalación escribe `CompassStamp` (mtime del exe en segundos). Si el exe
# cambió de mtime, la lista de herramientas o sus comandos pueden haber
# cambiado. Sin la marca (instalación anterior a ella) también se reinstala una
# vez; sin esto, un binario recompilado dejaba el menú viejo porque
# `01OpenCode` ya existía.
$menuVibeCoding = 1
if (Test-Path 'HKCU:\Software\Classes\Directory\ContextMenus\VibeCoding\shell\01OpenCode\command') {
    $menuVibeCoding = 0

    # Migración: las instalaciones anteriores abrían las herramientas con
    # `cmd.exe` (y arrastraban el icono clásico). Instalar pwsh *después* de
    # Compass no cambia el mtime del exe, así que la marca de abajo coincidiría
    # y el menú viejo se quedaría para siempre. Basta mirar la primera
    # sub-entrada: todas las escribe `install_vibe_coding()` en el mismo paso.
    $vibeCommand = (Get-ItemProperty `
        -Path 'HKCU:\Software\Classes\Directory\ContextMenus\VibeCoding\shell\01OpenCode\command').'(default)'
    if ($vibeCommand -like '*cmd.exe*') { $menuVibeCoding = 1 }

    $stampVibe = [string](Get-ItemProperty `
        -Path 'HKCU:\Software\Classes\Directory\ContextMenus\VibeCoding' `
        -Name 'CompassStamp' -ErrorAction SilentlyContinue).CompassStamp
    $exeCompass = Join-Path $env:USERPROFILE '.cargo\bin\compass.exe'
    if ([string]::IsNullOrEmpty($stampVibe)) {
        $menuVibeCoding = 1
    } elseif (Test-Path $exeCompass) {
        $epoch = [datetime]::SpecifyKind([datetime]'1970-01-01', [System.DateTimeKind]::Utc)
        # `Floor` y no `[long]` a secas: el cast de un double redondea y Rust
        # trunca (`as_secs`), así que con fracciones de segundo las dos marcas
        # diferían en 1 y el menú se reinstalaba siempre.
        $exeStamp = [string][long][math]::Floor(((Get-Item $exeCompass).LastWriteTimeUtc - $epoch).TotalSeconds)
        if ($stampVibe -ne $exeStamp) { $menuVibeCoding = 1 }
    }
}

# Herramientas de Vibe Coding ausentes. Se comprueba aquí, y no en un segundo
# script, por lo caro que es arrancar PowerShell 5.1: ya se está pagando el
# arranque, así que meter un `Get-Command` por herramienta sale gratis.
#
# `agy` y `hermes` necesitan el mismo fallback que `resolve_tool_command()` en
# Rust: sus instaladores nativos los dejan en `%LOCALAPPDATA%\<tool>\bin`, fuera
# del PATH, así que `Get-Command` no los ve aunque estén instalados.
# `kiro-cli` no sigue el patrón `LOCALAPPDATA\<herramienta>\bin`: su instalador
# lo deja en `LOCALAPPDATA\Kiro-Cli`, plano y con otro nombre de carpeta.
# `qodercli` instala bajo `%USERPROFILE%\.qoder`, también fuera del patrón.
$nombres = @{
    'opencode'  = 'opencode'
    'kilo'      = 'kilo'
    'cline'     = 'cline'
    'agy'       = 'agy'
    'hermes'    = 'hermes'
    'kiro-cli'  = 'Kiro-Cli'
    'copilot'   = 'copilot'
    'freebuff'  = 'freebuff'
    'qodercli'  = 'qodercli'
}

$faltan = 0
foreach ($herramienta in $nombres.Keys) {
    if (Get-Command -Name $herramienta -ErrorAction SilentlyContinue) { continue }
    # Copilot CLI se instala por winget, que no usa `%LOCALAPPDATA%\<tool>\bin`
    # sino su carpeta Links. Ahí está aunque el PATH de esta sesión no lo tenga.
    if (Test-Path (Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Links\$herramienta.exe")) { continue }
    # Qoder CLI deja el ejecutable bajo `%USERPROFILE%\.qoder` (y configura el
    # PATH para terminales nuevas, no para la que ejecuta `install.bat`).
    if ($herramienta -eq 'qodercli' -and
        (Test-Path (Join-Path $env:USERPROFILE '.qoder\bin\qodercli\qodercli.exe'))) { continue }
    $carpeta = $nombres[$herramienta]
    if (Test-Path (Join-Path $env:LOCALAPPDATA "$carpeta\bin\$herramienta.exe")) { continue }
    if (Test-Path (Join-Path $env:LOCALAPPDATA "$carpeta\$herramienta.exe")) { continue }
    $faltan++
}

Write-Output "$necesitaBuild;$menuCompass;$menuVibeCoding;$faltan"