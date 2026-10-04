<#
    Sonda de estado para install.bat / install.sh.

    Devuelve en una única línea, separada por punto y coma:

        <compilar>;<menu Code Runner>;<menu Vibe Coding>

    con 1 = hay que actuar y 0 = ya está al día.

    Va en un fichero y no en un one-liner dentro de `for /f` porque las tuberías
    (`|`) son caracteres especiales de `cmd`: rompen el comando del bucle. Y
    además PowerShell 5.1 tarda ~2,5 s en arrancar, así que conviene una sola
    invocación para las tres comprobaciones en vez de tres.

    Ante cualquier error devuelve "1;1;1": es decir, "reinstálalo todo", que es
    lo seguro. Saltarse una instalación que sí hacía falta deja el sistema
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

Write-Output "$necesitaBuild;$menuCodeRunner;$menuVibeCoding"