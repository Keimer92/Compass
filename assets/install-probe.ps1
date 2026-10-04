<#
    Sonda de estado para install.bat / install.sh.

    Devuelve en una única línea, separada por punto y coma:

        <compilar>;<menu CR>;<menu vibe>;<herramientas que faltan>

    con 1 = hay que actuar y 0 = ya está al día. El cuarto campo es un contador y
    no una lista: lo usa `install.bat` sólo para decidir si hay que lanzar la
    tarea que pregunta por las que faltan, y el detalle lo dice esa tarea.

    Va en un fichero y no en un one-liner dentro de `for /f` porque las tuberías
    (`|`) son caracteres especiales de `cmd`: rompen el comando del bucle. Y
    además PowerShell 5.1 tarda ~2,5 s en arrancar, así que conviene una sola
    invocación para todas las comprobaciones en vez de varias.

    Ante cualquier error devuelve "1;1;1;4": es decir, "reinstálalo todo", que
    es lo seguro. Saltarse una instalación que sí hacía falta deja el sistema
    incoherente; Installerla de más sólo cuesta unos segundos.
#>

$ErrorActionPreference = 'SilentlyContinue'

$necesitaBuild = 1
$exe = Join-Path $env:USERPROFILE '.cargo\bin\code-runner.exe'

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

# El menú de Code Runner es una entrada simple: `zz_CodeRunner` con el path del
# ejecutable en `(default)`. Si no está, o apunta a otro sitio, hay que rehacerlo.
$menuCodeRunner = 1
$comandoCr = (Get-ItemProperty `
    -Path 'HKCU:\Software\Classes\Directory\shell\zz_CodeRunner\command').'(default)'
if ($comandoCr -like '*code-runner.exe*') {
    $menuCodeRunner = 0
}

# El de Vibe Coding es cascada: la entrada padre `zz_VibeCoding` sólo lleva
# `ExtendedSubCommandsKey`, y las sub-entadas viven en otra ruta, con prefijo
# numérico (`01OpenCode`, `04Antigravity`...) porque Windows las ordena
# alfabéticamente. Hay que mirar ahí, no en `zz_VibeCoding\shell`.
$menuVibeCoding = 1
if (Test-Path 'HKCU:\Software\Classes\Directory\ContextMenus\VibeCoding\shell\01OpenCode\command') {
    $menuVibeCoding = 0
}

# Herramientas de Vibe Coding ausentes. Se comprueba aquí, y no en un segundo
# script, por lo caro que es arrancar PowerShell 5.1: ya se está pagando el
# arranque, así que meter un `Get-Command` por herramienta sale gratis.
#
# `agy` y `hermes` necesitan el mismo fallback que `resolve_tool_command()` en
# Rust: sus instaladores nativos los dejan en `%LOCALAPPDATA%\<tool>\bin`, fuera
# del PATH, así que `Get-Command` no los ve aunque estén instalados.
$faltan = 0
foreach ($herramienta in @('opencode', 'kilo', 'cline', 'agy', 'hermes')) {
    if (Get-Command -Name $herramienta -ErrorAction SilentlyContinue) { continue }
    if (Test-Path (Join-Path $env:LOCALAPPDATA "$herramienta\bin\$herramienta.exe")) { continue }
    $faltan++
}

Write-Output "$necesitaBuild;$menuCodeRunner;$menuVibeCoding;$faltan"