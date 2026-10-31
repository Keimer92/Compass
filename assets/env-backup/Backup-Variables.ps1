#Requires -Version 5.1
<#
    Backup-Variables.ps1
    Respaldo del entorno de desarrollo de Windows: variables de entorno (Sistema y
    Usuario), inventario de herramientas (Java, Flutter, Android, Rust, Kotlin,
    Gradle, .NET, Node, Python, Visual Studio, VS Code, Git, Chocolatey, Winget)
    y copia de los archivos de configuracion que no se pueden regenerar solos.

    Uso:
      PowerShell (como Administrador):  .\Backup-Variables.ps1
      Con destino personalizado:        .\Backup-Variables.ps1 -Destino "E:\Backups\Entorno"
      Sin preguntas interactivas:        .\Backup-Variables.ps1 -SinConfirmacion
      Solo inventario (no copia nada):   .\Backup-Variables.ps1 -SoloInventario

    Respaldos anteriores:
      Por defecto solo se conserva el mas reciente: al terminar, el script borra
      los EnvBackup_* anteriores de la carpeta de destino. La limpieza se hace
      DESPUES de crear el respaldo nuevo, nunca antes, para no quedarse sin
      ninguna copia si algo falla a mitad.
      Con -ConservarN 3 (o el numero que quieras) se dejan los 3 mas recientes.
      OJO: si un respaldo contiene una carpeta pre-restore_* (los .reg de
      rollback que crea Restore-Variables.ps1), se avisa antes de borrarlo.
#>
[CmdletBinding()]
param(
    [string]$Destino = "",
    [switch]$SinConfirmacion,
    [switch]$SoloInventario,
    [switch]$ConImagenesEmulador,
    [ValidateRange(1, 99)]
    [int]$ConservarN = 1
)

$ErrorActionPreference = "Stop"
$script:Inicio = Get-Date

# ============================================================================
#  HELPERS
# ============================================================================

$script:Resultados = New-Object System.Collections.Generic.List[object]

function Add-Resultado {
    param([string]$Area, [string]$Estado, [string]$Detalle = "")
    $script:Resultados.Add([pscustomobject]@{ Area = $Area; Estado = $Estado; Detalle = $Detalle })
    $color = switch ($Estado) {
        "OK"      { "DarkGreen" }
        "AVISO"   { "Yellow" }
        "FALLO"   { "Red" }
        default   { "DarkGray" }
    }
    $marca = switch ($Estado) {
        "OK"      { "[OK]    " }
        "AVISO"   { "[AVISO] " }
        "FALLO"   { "[FALLO] " }
        default   { "[--]    " }
    }
    Write-Host ("  {0} {1,-30} {2}" -f $marca, $Area, $Detalle) -ForegroundColor $color
}

function Write-Titulo {
    param([string]$Texto)
    Write-Host ""
    Write-Host "--- $Texto ---" -ForegroundColor Cyan
}

# Ejecuta un proceso nativo capturando stdout Y stderr sin que PowerShell 5.1
# convierta la salida de error en una excepcion terminante (bug del script
# anterior: `$ErrorActionPreference = "Stop"` + `2>&1` abortaba el respaldo).
#   -OutFile  : redirige la salida combinada a un archivo UTF-8
#   -sin OutFile : devuelve un objeto con ExitCode y Text
#
# Timeout: antes se usaba Start-Process -Wait, que espera PARA SIEMPRE. Con eso
# cualquier herramienta que se cuelgue (flutter doctor hace llamadas de red y
# espera al ADB) dejaba el backup colgado indefinidamente. Ahora se espera con
# limite y, si se pasa, se mata el proceso y se sigue adelante: un inventario
# incompleto es mejor que un backup que nunca termina.
function Invoke-Native {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @(),
        [string]$OutFile,
        [int]$TimeoutSeg = 300
    )

    $oldEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        if (-not $OutFile) {
            $texto = (& $FilePath @Arguments 2>&1 | Out-String)
            $code = $LASTEXITCODE
            return [pscustomobject]@{ ExitCode = $code; Text = $texto; Ok = ($code -eq 0) }
        }

        $archivoErr = "$OutFile.stderr.tmp"
        # -Wait NO: se espera con limite mediante WaitForExit(milisegundos).
        $proc = Start-Process -FilePath $FilePath -ArgumentList $Arguments `
                    -NoNewWindow -PassThru `
                    -RedirectStandardOutput $OutFile -RedirectStandardError $archivoErr `
                    -ErrorAction Stop

        # IMPRESCINDIBLE: leer .Handle cachea el handle del proceso. Sin esto,
        # .ExitCode devuelve NULL en cuanto el proceso termina, porque .NET lo
        # necesita para consultar el codigo de salida. Con -Wait no pasaba porque
        # PowerShell conservaba el handle; al quitar -Wait (para poder poner un
        # timeout) hay que cachearlo a mano o todos los comandos salen con
        # "exit " vacio y el inventario entero se marca como AVISO.
        $null = $proc.Handle

        $terminado = $proc.WaitForExit($TimeoutSeg * 1000)
        if (-not $terminado) {
            try { $proc.Kill() } catch { }
            try { $proc.WaitForExit(5000) | Out-Null } catch { }
            Set-Content -LiteralPath $OutFile -Encoding utf8 -Value @(
                "[Invoke-Native] TIMEOUT: '$FilePath $($Arguments -join ' ')' no termino en $TimeoutSeg s y se ha terminado.",
                "Suele ser una herramienta esperando red o un dispositivo (flutter doctor, adb, sdkmanager).",
                "No es un fallo del respaldo: el resto del inventario se guarda igualmente."
            )
            Remove-Item -LiteralPath $archivoErr -Force -ErrorAction SilentlyContinue
            return [pscustomobject]@{ ExitCode = -2; Text = "timeout tras $TimeoutSeg s"; Ok = $false }
        }
        # Sin esta segunda llamada, con la salida redirigida .NET puede devolver
        # antes de vaciar los buffers y se pierde el final de la salida.
        try { $proc.WaitForExit() } catch { }

        $code = $proc.ExitCode
        $err = ""
        if (Test-Path -LiteralPath $archivoErr) {
            $err = [string](Get-Content -LiteralPath $archivoErr -Raw -ErrorAction SilentlyContinue)
            Remove-Item -LiteralPath $archivoErr -Force -ErrorAction SilentlyContinue
        }
    } catch {
        $ErrorActionPreference = $oldEap
        Set-Content -LiteralPath $OutFile -Value ("[Invoke-Native] ERROR ejecutando '$FilePath $($Arguments -join ' ')': $($_.Exception.Message)") -Encoding utf8
        return [pscustomobject]@{ ExitCode = -1; Text = $_.Exception.Message; Ok = $false }
    } finally {
        $ErrorActionPreference = $oldEap
    }

    if ($err -and $err.Trim()) {
        Add-Content -LiteralPath $OutFile -Value "`n----- STDERR -----`n$err" -Encoding utf8
    }
    return [pscustomobject]@{ ExitCode = $code; Text = $err; Ok = ($code -eq 0) }
}

function Test-Comando {
    param([string]$Nombre)
    return [bool](Get-Command $Nombre -ErrorAction SilentlyContinue)
}

# Start-Process no puede ejecutar scripts .ps1 (npm, choco, etc. suelen ser wrappers).
# Resolvemos al .cmd / .exe equivalente.
function Resolve-Comando {
    param([string]$Nombre)
    $cmd = Get-Command $Nombre -ErrorAction SilentlyContinue
    if (-not $cmd) { return $null }
    if ($cmd.CommandType -eq "Application") { return $cmd.Source }
    $base = Join-Path (Split-Path $cmd.Source -Parent) $Nombre
    foreach ($ext in @(".cmd", ".exe", ".bat")) {
        $p = $base + $ext
        if (Test-Path -LiteralPath $p) { return $p }
    }
    return $cmd.Source
}

function Save-Texto {
    param([string]$Contenido, [string]$Ruta)
    Set-Content -LiteralPath $Ruta -Value $Contenido -Encoding utf8
}

function Save-Resultado {
    # Guarda la salida de una herramienta. Nunca aborta el respaldo.
    param(
        [string]$Area,
        [string]$Comando,
        [string[]]$Argumentos,
        [string]$Archivo,
        [switch]$Opcional
    )
    $exe = Resolve-Comando $Comando
    if (-not $exe) {
        Add-Resultado $Area "OMITIDO" "$Comando no instalado"
        return
    }
    $r = Invoke-Native -FilePath $exe -Arguments $Argumentos -OutFile $Archivo
    if ($r.Ok) {
        $tam = (Get-Item -LiteralPath $Archivo -ErrorAction SilentlyContinue).Length
        Add-Resultado $Area "OK" "$([System.IO.Path]::GetFileName($Archivo)) ($tam bytes)"
    } else {
        Add-Resultado $Area "AVISO" "$([System.IO.Path]::GetFileName($Archivo)) - exit $($r.ExitCode)"
    }
}

function Copy-Archivo {
    param([string]$Area, [string]$Origen, [string]$DestinoRelativo, [string]$CarpetaOut)
    if (-not (Test-Path -LiteralPath $Origen -PathType Leaf)) {
        Add-Resultado $Area "OMITIDO" "no existe: $Origen"
        return
    }
    try {
        $dest = Join-Path $CarpetaOut $DestinoRelativo
        $padre = Split-Path $dest -Parent
        if (-not (Test-Path -LiteralPath $padre)) { New-Item -ItemType Directory -Path $padre -Force | Out-Null }
        Copy-Item -LiteralPath $Origen -Destination $dest -Force
        Add-Resultado $Area "OK" "$DestinoRelativo"
    } catch {
        Add-Resultado $Area "FALLO" "$DestinoRelativo : $($_.Exception.Message)"
    }
}

function Copy-Arbol {
    param(
        [string]$Area,
        [string]$Origen,
        [string]$DestinoRelativo,
        [string]$CarpetaOut,
        [string[]]$ExcluirDir = @(),
        [string[]]$ExcluirArchivo = @()
    )
    if (-not (Test-Path -LiteralPath $Origen -PathType Container)) {
        Add-Resultado $Area "OMITIDO" "no existe: $Origen"
        return
    }
    try {
        $dest = Join-Path $CarpetaOut $DestinoRelativo
        New-Item -ItemType Directory -Path $dest -Force | Out-Null
        # OJO: robocopy excluye CARPETAS con /XD y ARCHIVOS con /XF.
        # Poner un nombre de carpeta en /XF no la excluye y acabas copiando cientos de MB de basura.
        $robArgs = @($Origen, $dest, '/E', '/R:1', '/W:1', '/NFL', '/NDL', '/NJH', '/NJS', '/NP', '/XJ')
        if ($ExcluirDir.Count -gt 0)    { $robArgs += '/XD'; $robArgs += $ExcluirDir }
        if ($ExcluirArchivo.Count -gt 0) { $robArgs += '/XF'; $robArgs += $ExcluirArchivo }
        $oldEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try { & robocopy.exe @robArgs | Out-Null; $code = $LASTEXITCODE }
        finally { $ErrorActionPreference = $oldEap }
        if ($code -le 7) {
            $m = Get-ChildItem -LiteralPath $dest -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Sum Length
            Add-Resultado $Area "OK" ("$($m.Count) archivos, {0:N1} MB" -f ($m.Sum / 1MB))
        } else {
            Add-Resultado $Area "AVISO" "robocopy exit $code"
        }
    } catch {
        Add-Resultado $Area "FALLO" "$DestinoRelativo : $($_.Exception.Message)"
    }
}

function Get-FormatoBytes {
    param([double]$Bytes)
    if ($Bytes -ge 1GB) { return "{0:N2} GB" -f ($Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return "{0:N1} MB" -f ($Bytes / 1MB) }
    if ($Bytes -ge 1KB) { return "{0:N1} KB" -f ($Bytes / 1KB) }
    return "$Bytes B"
}

# ============================================================================
#  0. CARPETA DE DESTINO
# ============================================================================

if ([string]::IsNullOrWhiteSpace($Destino)) {
    $Destino = Join-Path $PSScriptRoot "Backup"
}

$timestamp  = Get-Date -Format "yyyy-MM-dd_HHmmss"
# Concatenación y no `Join-Path`: con una unidad inexistente `Join-Path` lanza
# "Cannot find drive" y el aviso de abajo ('NO existe') nunca se mostraba.
$carpetaOut = "$($Destino.TrimEnd('\'))\EnvBackup_$timestamp"

Write-Host ""
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  BACKUP DEL ENTORNO DE DESARROLLO" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  Destino : $carpetaOut"
Write-Host "  Inicio  : $($script:Inicio)"
Write-Host "  Modo    : $(if ($SoloInventario) { 'solo inventario (no copia archivos)' } else { 'completo' })"

# Verifica que la unidad del destino exista
$unidadDestino = (Split-Path -Path $carpetaOut -Qualifier)
if ($unidadDestino) {
    $raizDestino = "$unidadDestino\"
    if (-not (Test-Path -LiteralPath $raizDestino)) {
        Write-Host ""
        Write-Host "[FALLO] La unidad '$unidadDestino' del destino NO existe." -ForegroundColor Red
        exit 1
    }
    if ($raizDestino.TrimEnd('\') -ieq 'C:') {
        Write-Host ""
        Write-Host "[AVISO] El destino esta en C:, la unidad que probablemente vas a formatear." -ForegroundColor Yellow
        Write-Host "        Este backup se PERDERA. Copialo a otra unidad (D:, E:, USB) o comprimilo." -ForegroundColor Yellow
        if (-not $SinConfirmacion) {
            $ok = Read-Host "  Continuar de todos modos? (s/N)"
            if ($ok -ne "s" -and $ok -ne "S") { Write-Host "Cancelado."; Write-Host ""; exit 1 }
        }
    }
}

if (-not (Test-Path -LiteralPath $carpetaOut)) {
    New-Item -ItemType Directory -Path $carpetaOut -Force | Out-Null
}

$dirInventario = Join-Path $carpetaOut "inventario"
$dirArchivos   = Join-Path $carpetaOut "archivos"
New-Item -ItemType Directory -Path $dirInventario -Force | Out-Null
New-Item -ItemType Directory -Path $dirArchivos   -Force | Out-Null

$regSistemaPath = Join-Path $carpetaOut "env-SISTEMA.reg"
$regUsuarioPath = Join-Path $carpetaOut "env-USUARIO.reg"

# ============================================================================
#  1. VARIABLES DE ENTORNO (LO MAS IMPORTANTE)
# ============================================================================

Write-Titulo "1. Variables de entorno"

$okSistema = $false
try {
    reg.exe export "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "$regSistemaPath" /y | Out-Null
    if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $regSistemaPath)) { $okSistema = $true }
} catch { }

if ($okSistema) {
    Add-Resultado "env-SISTEMA.reg" "OK" ("{0} - variables de SISTEMA (requiere Admin)" -f (Get-FormatoBytes (Get-Item -LiteralPath $regSistemaPath).Length))
} else {
    Add-Resultado "env-SISTEMA.reg" "FALLO" "no se pudo exportar. Ejecuta PowerShell como ADMINISTRADOR"
}

$okUsuario = $false
try {
    reg.exe export "HKCU\Environment" "$regUsuarioPath" /y | Out-Null
    if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $regUsuarioPath)) { $okUsuario = $true }
} catch { }

if ($okUsuario) {
    Add-Resultado "env-USUARIO.reg" "OK" ("{0} - variables de USUARIO" -f (Get-FormatoBytes (Get-Item -LiteralPath $regUsuarioPath).Length))
} else {
    Add-Resultado "env-USUARIO.reg" "FALLO" "no se pudo exportar"
}

if (-not $okSistema -and -not $okUsuario) {
    Write-Host ""
    Write-Host "[FALLO] No se pudo exportar ninguna variable. Abortando." -ForegroundColor Red
    exit 1
}

# Copia legible y fiel: conserva el valor SIN expandir (REG_EXPAND_SZ tal cual)
function Write-VariablesLegibles {
    param([string]$Etiqueta, [string]$ClaveRegistro, [string]$ArchivoDestino)
    $lineas = New-Object System.Collections.Generic.List[string]
    $lineas.Add("=== VARIABLES DE $Etiqueta ($ClaveRegistro) ===")
    $lineas.Add("Generado: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')")
    $lineas.Add("")
    $lineas.Add("--- Valores EXACTOS (sin expandir) - estos son los que se restauran ---")
    $clave = Get-Item -LiteralPath $ClaveRegistro
    foreach ($nombre in $clave.GetValueNames()) {
        $bruto = $clave.GetValue($nombre, "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $tipo  = $clave.GetValueKind($nombre)
        $lineas.Add(("{0,-24} [{1,-16}] = {2}" -f $nombre, $tipo, $bruto))
    }
    $lineas.Add("")
    $lineas.Add("--- Valores EXPANDIDOS (solo informativos) ---")
    foreach ($nombre in $clave.GetValueNames()) {
        $lineas.Add(("{0,-24} = {1}" -f $nombre, $clave.GetValue($nombre)))
    }
    Save-Texto ($lineas -join "`r`n") $ArchivoDestino
}

try {
    Write-VariablesLegibles -Etiqueta "SISTEMA" -ClaveRegistro "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment" -ArchivoDestino (Join-Path $carpetaOut "env-SISTEMA-legible.txt")
    Add-Resultado "env-SISTEMA-legible.txt" "OK" "volcado legible del sistema"
} catch { Add-Resultado "env-SISTEMA-legible.txt" "AVISO" $_.Exception.Message }

try {
    Write-VariablesLegibles -Etiqueta "USUARIO" -ClaveRegistro "HKCU:\Environment" -ArchivoDestino (Join-Path $carpetaOut "env-USUARIO-legible.txt")
    Add-Resultado "env-USUARIO-legible.txt" "OK" "volcado legible del usuario"
} catch { Add-Resultado "env-USUARIO-legible.txt" "AVISO" $_.Exception.Message }

# El PATH de USUARIO deberia ser REG_EXPAND_SZ. Si algo lo dejo como REG_SZ (el
# sospechoso clasico es [Environment]::SetEnvironmentVariable, que SIEMPRE escribe
# REG_SZ), las referencias %VAR% que se anadan en el futuro se quedaran como texto
# literal. Con el PATH actual da igual porque no usa %VAR%, pero conviene saberlo
# ANTES de que alguien agregue, por ejemplo, %USERPROFILE%\bin.
try {
    $kUser   = Get-Item -LiteralPath "HKCU:\Environment"
    $kindPath = $kUser.GetValueKind("Path")
    if ($kindPath -eq [Microsoft.Win32.RegistryValueKind]::String) {
        $rawUser = $kUser.GetValue("Path", "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $refs = @([regex]::Matches($rawUser, '%[^%]+%') | ForEach-Object { $_.Value } | Sort-Object -Unique)
        if ($refs.Count -gt 0) {
            Add-Resultado "PATH de USUARIO" "AVISO" "es REG_SZ y usa $($refs -join ', '): esas referencias NO se expanden. Deberia ser REG_EXPAND_SZ"
        } else {
            Add-Resultado "PATH de USUARIO" "AVISO" "es REG_SZ en vez de REG_EXPAND_SZ. Inofensivo hoy (no usa %VAR%), pero un %VAR% anadido despues no se expandira"
        }
    }
} catch { }

# Otras claves de registro utiles tras un formateo
Write-Titulo "2. Otras claves de registro utiles"
$clavesExtra = @(
    @{ N = "carpetas-conocidas.reg"; K = "HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders"; D = "carpetas del usuario" },
    @{ N = "compat-elevacion.reg"; K = "HKCU\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers"; D = "programas configurados para ejecutar como admin" },
    @{ N = "idioma-regional.reg";  K = "HKCU\Control Panel\International";                                       D = "formato regional y teclado" }
)
foreach ($c in $clavesExtra) {
    $dest = Join-Path $carpetaOut $c.N
    try {
        reg.exe export $c.K "$dest" /y | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $dest)) { Add-Resultado $c.N "OK" $c.D }
        else { Add-Resultado $c.N "AVISO" "clave no disponible" }
    } catch { Add-Resultado $c.N "AVISO" $_.Exception.Message }
}

# ============================================================================
#  3. INVENTARIO DE HERRAMIENTAS
# ============================================================================

Write-Titulo "3. Inventario de herramientas"

# --- Java / JDK ---
Save-Resultado "java" "java" @('-version') (Join-Path $dirInventario "java-version.txt")
Save-Resultado "javac" "javac" @('-version') (Join-Path $dirInventario "javac-version.txt")

$jdkDirs = @()
foreach ($base in @("C:\Program Files\Java", "C:\Program Files\Eclipse Adoptium", "C:\Program Files\Microsoft\jdk")) {
    if (Test-Path -LiteralPath $base) {
        $jdkDirs += (Get-ChildItem -LiteralPath $base -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
    }
}
$jdksUsuario = Join-Path $env:USERPROFILE ".jdks"
if (Test-Path -LiteralPath $jdksUsuario) {
    $jdkDirs += (Get-ChildItem -LiteralPath $jdksUsuario -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
}
if ($jdkDirs.Count -gt 0) {
    Save-Texto ($jdkDirs -join "`r`n") (Join-Path $dirInventario "jdk-instalados.txt")
    Add-Resultado "JDK instalados" "OK" "$($jdkDirs.Count) carpeta(s) - jdk-instalados.txt"
} else {
    Save-Texto "(ninguna carpeta de JDK detectada)" (Join-Path $dirInventario "jdk-instalados.txt")
    Add-Resultado "JDK instalados" "OMITIDO" "no se detectaron carpetas de JDK"
}

# --- Kotlin / Gradle ---
# Ni Kotlin ni Gradle se suelen instalar como programas globales:
#   - Gradle se usa mediante el Gradle Wrapper (gradlew) de cada proyecto, que
#     descarga la distribucion sola a GRADLE_USER_HOME\wrapper\dists.
#   - Kotlin llega como plugin de Gradle (org.jetbrains.kotlin:...), no como kotlinc.
# Que no aparezcan en el PATH es lo NORMAL, no un fallo del backup.
$gradleHome = [Environment]::GetEnvironmentVariable("GRADLE_USER_HOME", "User")
if ([string]::IsNullOrWhiteSpace($gradleHome)) { $gradleHome = [Environment]::GetEnvironmentVariable("GRADLE_USER_HOME", "Machine") }
if ([string]::IsNullOrWhiteSpace($gradleHome)) { $gradleHome = Join-Path $env:USERPROFILE ".gradle" }

if (Test-Comando "kotlinc") {
    Save-Resultado "kotlinc" "kotlinc" @('-version') (Join-Path $dirInventario "kotlinc-version.txt")
} else {
    Add-Resultado "kotlinc" "OMITIDO" "sin compilador global (normal: Kotlin llega por Gradle)"
}
if (Test-Comando "gradle") {
    Save-Resultado "gradle" "gradle" @('--version') (Join-Path $dirInventario "gradle-version.txt")
} else {
    Add-Resultado "gradle" "OMITIDO" "sin instalacion global (normal: se usa el Gradle Wrapper)"
}

$lineasGradle = New-Object System.Collections.Generic.List[string]
$lineasGradle.Add("GRADLE_USER_HOME = $gradleHome")
$lineasGradle.Add("Generado: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')")
$lineasGradle.Add("")

$distDir = Join-Path $gradleHome "wrapper\dists"
if (Test-Path -LiteralPath $distDir) {
    $lineasGradle.Add("DISTRIBUCIONES DE GRADLE DESCARGADAS POR EL WRAPPER")
    foreach ($d in (Get-ChildItem -LiteralPath $distDir -Directory -ErrorAction SilentlyContinue)) {
        $lineasGradle.Add("  $($d.Name)")
    }
    $lineasGradle.Add("")
    $lineasGradle.Add("  (cada proyecto usa una de estas segun su gradle/wrapper/gradle-wrapper.properties)")
} else {
    $lineasGradle.Add("No hay distribuciones descargadas en wrapper\dists")
}
$lineasGradle.Add("")

# JDK que Gradle puede descargar para los toolchains declarados en build.gradle
$jdkGradle = Join-Path $gradleHome "jdks"
if (Test-Path -LiteralPath $jdkGradle) {
    $jds = @(Get-ChildItem -LiteralPath $jdkGradle -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name)
    if ($jds.Count -gt 0) {
        $lineasGradle.Add("JDK DESCARGADOS POR GRADLE (toolchains):")
        $jds | ForEach-Object { $lineasGradle.Add("  $_") }
        $lineasGradle.Add("")
    }
}

$cacheModulos = Join-Path $gradleHome "caches\modules-2\files-2.1"
if (Test-Path -LiteralPath $cacheModulos) {
    $grupos = @(Get-ChildItem -LiteralPath $cacheModulos -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name | Select-Object -ExpandProperty Name)
    $lineasGradle.Add("GRUPOS EN LA CACHE DE MODULOS: $($grupos.Count)")
    $lineasGradle.Add("  (lista resumida; las VERSIONES que importan estan en kotlin-android-stack.txt)")
    $grupos | ForEach-Object { $lineasGradle.Add("  $_") }
} else {
    $lineasGradle.Add("No hay cache de modulos (aun no has construido ningun proyecto Gradle)")
}

Save-Texto ($lineasGradle -join "`r`n") (Join-Path $dirInventario "gradle-estado.txt")
Add-Resultado "gradle-estado.txt" "OK" "GRADLE_USER_HOME, distribuciones del wrapper y grupos"

# Versiones concretas de Kotlin / AGP / Compose / KMP de la cache de Gradle.
# Es la informacion que de verdad hace falta para reconstruir un proyecto Kotlin.
function Get-OrdenVersiones {
    param([string[]]$Versiones)
    $pares = New-Object System.Collections.Generic.List[object]
    foreach ($v in $Versiones) {
        $m = [regex]::Match($v, '^(\d+(?:\.\d+)*)')
        $clave = if ($m.Success) {
            (($m.Groups[1].Value -split '\.') | ForEach-Object { ([int]$_).ToString().PadLeft(10, '0') }) -join '.'
        } else { "0000000000" }
        $pares.Add([pscustomobject]@{ Txt = $v; Clave = $clave })
    }
    return @($pares | Sort-Object -Property Clave -Descending | Select-Object -ExpandProperty Txt)
}

function Write-StackKotlin {
    param([string]$CacheRoot, [string]$Archivo, [string]$GradleHomeTxt)
    $secciones = @(
        @{ Titulo = "PLUGIN DE KOTLIN (Kotlin Gradle Plugin)"; Grupos = @("org.jetbrains.kotlin") },
        @{ Titulo = "ANDROID GRADLE PLUGIN (AGP)";             Grupos = @("com.android.tools.build", "com.android.application") },
        @{ Titulo = "COMPOSE MULTIPLATFORM";                    Grupos = @("org.jetbrains.compose", "org.jetbrains.compose.runtime", "org.jetbrains.compose.foundation") },
        @{ Titulo = "KOTLINX (coroutines, serialization...)";   Grupos = @("org.jetbrains.kotlinx") },
        @{ Titulo = "KOTLIN MULTIPLATFORM / NATIVE";            Grupos = @("org.jetbrains.kotlin.native", "org.jetbrains.kotlin.multiplatform") },
        @{ Titulo = "GOOGLE PLAY SERVICES / FIREBASE";          Grupos = @("com.google.android.gms", "com.google.firebase") },
        @{ Titulo = "OTRAS LIBRERIAS DE TERCEROS";              Grupos = @("io.ktor", "co.touchlab", "com.squareup.okhttp3", "com.google.dagger", "app.cash.sqldelight", "com.russhwolf") }
    )
    $lineas = New-Object System.Collections.Generic.List[string]
    $lineas.Add("VERSIONES DE KOTLIN / ANDROID / COMPOSE DESCARGADAS")
    $lineas.Add("=================================================")
    $lineas.Add("Generado    : $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')")
    $lineas.Add("Cache de    : $CacheRoot")
    $lineas.Add("Gradle home : $GradleHomeTxt")
    $lineas.Add("")
    $lineas.Add("Estas versiones son las que tenian tus proyectos. Para reconstruir uno, mira su")
    $lineas.Add("build.gradle(.kts) y usa las mismas. Se redescargan solas de Maven Central.")
    $lineas.Add("")

    $total = 0
    # Primero se recogen las claves para poder alinear la columna de versiones
    $filas = @()
    foreach ($s in $secciones) {
        foreach ($g in $s.Grupos) {
            $dirG = Join-Path $CacheRoot $g
            if (-not (Test-Path -LiteralPath $dirG)) { continue }
            foreach ($artefacto in (Get-ChildItem -LiteralPath $dirG -Directory -ErrorAction SilentlyContinue | Sort-Object Name)) {
                $versiones = @(Get-ChildItem -LiteralPath $artefacto.FullName -Directory -ErrorAction SilentlyContinue |
                               Select-Object -ExpandProperty Name)
                if ($versiones.Count -eq 0) { continue }
                $filas += [pscustomobject]@{ Seccion = $s.Titulo; Clave = ("{0}:{1}" -f $g, $artefacto.Name); Versiones = (Get-OrdenVersiones $versiones) }
            }
        }
    }
    $ancho = 0
    foreach ($f in $filas) { if ($f.Clave.Length -gt $ancho) { $ancho = $f.Clave.Length } }
    $ancho = [Math]::Min([Math]::Max($ancho + 2, 50), 74)

    foreach ($s in $secciones) {
        $lineas.Add($s.Titulo)
        $lineas.Add("-" * $s.Titulo.Length)
        $propias = @($filas | Where-Object { $_.Seccion -eq $s.Titulo })
        if ($propias.Count -eq 0) {
            $lineas.Add("  (no hay ninguno)")
        } else {
            foreach ($f in $propias) {
                $lineas.Add("  " + $f.Clave.PadRight($ancho) + ($f.Versiones -join ', '))
            }
        }
        $lineas.Add("")
        $total += $propias.Count
    }
    Save-Texto ($lineas -join "`r`n") $Archivo
    return $total
}

if (Test-Path -LiteralPath $cacheModulos) {
    try {
        $n = Write-StackKotlin -CacheRoot $cacheModulos -Archivo (Join-Path $dirInventario "kotlin-android-stack.txt") -GradleHomeTxt $gradleHome
        Add-Resultado "kotlin-android-stack.txt" "OK" "$n modulo(s) con su version"
    } catch {
        Add-Resultado "kotlin-android-stack.txt" "AVISO" $_.Exception.Message
    }
} else {
    Add-Resultado "kotlin-android-stack.txt" "OMITIDO" "no hay cache de Gradle que inspeccionar"
}

# --- Flutter / Dart ---
if (Test-Comando "flutter") {
    Save-Resultado "flutter version" "flutter" @('--version', '--machine') (Join-Path $dirInventario "flutter-version.txt")
    Save-Resultado "flutter config" "flutter" @('config', '--list') (Join-Path $dirInventario "flutter-config.txt")
    # flutter doctor hace llamadas de red y espera al ADB: es la herramienta que
    # mas se cuelga. Timeout corto a proposito; Invoke-Native lo mata y continua.
    $fDoctor = Join-Path $dirInventario "flutter-doctor.txt"
    $r = Invoke-Native -FilePath (Resolve-Comando "flutter") -Arguments @('doctor', '-v') -OutFile $fDoctor -TimeoutSeg 120
    if ($r.ExitCode -eq -2) {
        Add-Resultado "flutter doctor" "AVISO" "se quedo colgado, se cancelo a los 120 s (no afecta al resto del respaldo)"
    } elseif ($r.Ok) {
        Add-Resultado "flutter doctor" "OK" "$(Split-Path $fDoctor -Leaf) ($((Get-Item -LiteralPath $fDoctor).Length) bytes)"
    } else {
        Add-Resultado "flutter doctor" "AVISO" "flutter doctor fallo (exit $($r.ExitCode))"
    }
    Add-Resultado "flutter" "OK" "version, config y doctor guardados"
} else {
    Add-Resultado "flutter" "OMITIDO" "flutter no esta en el PATH"
}
Save-Resultado "dart" "dart" @('--version') (Join-Path $dirInventario "dart-version.txt")

# --- Android ---
$androidHome = [Environment]::GetEnvironmentVariable("ANDROID_HOME", "User")
if ([string]::IsNullOrWhiteSpace($androidHome)) { $androidHome = [Environment]::GetEnvironmentVariable("ANDROID_HOME", "Machine") }
$androidRoot = [Environment]::GetEnvironmentVariable("ANDROID_SDK_ROOT", "User")
if ([string]::IsNullOrWhiteSpace($androidRoot)) { $androidRoot = $androidHome }

if (Test-Comando "sdkmanager") {
    Save-Resultado "sdkmanager" "sdkmanager" @('--list_installed') (Join-Path $dirInventario "android-paquetes-instalados.txt")
} elseif ($androidRoot -and (Test-Path -LiteralPath (Join-Path $androidRoot "cmdline-tools"))) {
    Add-Resultado "sdkmanager" "OMITIDO" "existe la carpeta pero no esta en el PATH"
} else {
    Add-Resultado "sdkmanager" "OMITIDO" "SDK de Android no detectado"
}
Save-Resultado "adb" "adb" @('version') (Join-Path $dirInventario "adb-version.txt")

if ($androidRoot -and (Test-Path -LiteralPath $androidRoot)) {
    $lineasSdk = @("ANDROID_HOME   = $androidHome", "ANDROID_SDK_ROOT= $androidRoot", "")
    foreach ($sub in @("platforms", "build-tools", "platform-tools", "ndk", "cmdline-tools", "system-images", "sources")) {
        $p = Join-Path $androidRoot $sub
        if (Test-Path -LiteralPath $p) {
            $items = Get-ChildItem -LiteralPath $p -ErrorAction SilentlyContinue |
                     Where-Object { $_.Name -ne 'package.xml' } | Select-Object -ExpandProperty Name
            $lineasSdk += ("{0,-16}: {1}" -f $sub, ($items -join ', '))
        }
    }
    Save-Texto ($lineasSdk -join "`r`n") (Join-Path $dirInventario "android-sdk-contenido.txt")
    Add-Resultado "android-sdk-contenido.txt" "OK" "versiones instaladas en $androidRoot"
} else {
    Add-Resultado "android-sdk-contenido.txt" "OMITIDO" "ANDROID_HOME / ANDROID_SDK_ROOT no definidos o inexistentes"
}

# --- NDK: toolchain LLVM que necesita cargo/rustc para compilar para Android ---
# Tauri (y Rust en general) NO usa el clang de Android Studio: usa los clang
# "tripled" del NDK, que viven en
#     <NDK>\toolchains\llvm\prebuilt\<host>\bin\   (aarch64-linux-androidNN-clang.cmd...)
# Si esa carpeta NO esta en el PATH, 'tauri android build' / 'cargo build --target
# aarch64-linux-android' falla al compilar el lib. Por eso se comprueba y se avisa:
# android-sdk-contenido.txt solo decia QUE version de NDK hay, no si su toolchain
# era utilizable.
#
# OJO con la deriva de versiones: la entrada del PATH lleva la version del NDK
# escrita a mano (...ndk\27.0.12077973\toolchains\...). Si actualizas el NDK, esa
# entrada se queda obsoleta y hay que volver a anadirla con la version nueva.
$carpetasNdk = @()
if ($androidRoot) {
    foreach ($sub in @("ndk", "ndk-bundle")) {
        $dir = Join-Path $androidRoot $sub
        if (Test-Path -LiteralPath $dir) {
            $carpetasNdk += @(Get-ChildItem -LiteralPath $dir -Directory -ErrorAction SilentlyContinue |
                              Select-Object -ExpandProperty FullName)
        }
    }
}

# El PATH real (tal como lo vera cargo), no el del proceso actual.
$entradasPath = @((([Environment]::GetEnvironmentVariable("Path", "User")) + ";" +
                    ([Environment]::GetEnvironmentVariable("Path", "Machine"))) -split ";" |
                  ForEach-Object { $_.Trim().TrimEnd('\') })

# NDK "activo": el al que apuntan ANDROID_NDK_HOME o NDK_HOME. Es el que usan de
# verdad Tauri y Gradle. Puede haber mas NDK instalados (y ser mas nuevos), pero si
# el activo no tiene su toolchain en el PATH, Android no compila. Advertencia: que
# un NDK inactivo no este en el PATH es NORMAL y no hay que arreglarlo.
$ndkActivo = ""
foreach ($var in @("ANDROID_NDK_HOME", "NDK_HOME")) {
    foreach ($amb in @("User", "Machine")) {
        $v = [Environment]::GetEnvironmentVariable($var, $amb)
        if (-not [string]::IsNullOrWhiteSpace($v)) { $ndkActivo = $v.TrimEnd('\'); break }
    }
    if ($ndkActivo) { break }
}
$carpetasNdkOrdenadas = @()
if ($ndkActivo -and @($carpetasNdk | Where-Object { $_.TrimEnd('\') -ieq $ndkActivo }).Count -gt 0) {
    $carpetasNdkOrdenadas += $ndkActivo
}
# OJO: el NDK en uso va primero; el resto se ordenan de mas nuevo a mas viejo.
# Sin -Property, porque los elementos son CADENAS y no tienen propiedad 'Name':
# con -Property Name todas las claves salen a null y -Descending solo invierte el
# orden original, que es justo lo que no queremos.
# Tampoco [System.Version]::Parse(($x) -replace 'a','b'): el -replace con coma
# devuelve un ARRAY y Parse lo recibe como dos argumentos.
$carpetasNdkOrdenadas += @($carpetasNdk |
                           Where-Object { -not $ndkActivo -or $_.TrimEnd('\') -ine $ndkActivo } |
                           Sort-Object -Descending)

$lineasNdk = New-Object System.Collections.Generic.List[string]
$lineasNdk.Add("TOOLCHAIN LLVM DEL NDK (necesaria para compilar Rust / Tauri en Android)")
$lineasNdk.Add("=" * 74)
$lineasNdk.Add("Generado: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')")
$lineasNdk.Add("")
$lineasNdk.Add("NDK EN USO (el que usan Tauri y Gradle):")
$lineasNdk.Add("  ANDROID_NDK_HOME = $(if ($ndkActivo) { $ndkActivo } else { '(sin definir)' })")
$lineasNdk.Add("")
$lineasNdk.Add("  Si instalas un NDK mas nuevo que este, las variables siguen apuntando al")
$lineasNdk.Add("  viejo hasta que las cambies a mano. Comprueba que sea el que quieres.")
$lineasNdk.Add("")

$nDirsBin    = 0
$nEnPath     = 0
$faltan      = @()          # toolchains ausentes del PATH
$faltanCrit  = @()          # de esas, las del NDK EN USO

if ($carpetasNdk.Count -eq 0) {
    $lineasNdk.Add("NDKS INSTALADOS: ninguno (no existe $androidRoot\ndk)")
    $lineasNdk.Add("")
    $lineasNdk.Add("Para compilar Rust/Tauri en Android hace falta uno:")
    $lineasNdk.Add("  sdkmanager --sdk_root=`"$androidRoot`" `"ndk;27.0.12077973`"")
} else {
    $lineasNdk.Add("NDKS INSTALADOS")
    $lineasNdk.Add("-" * 74)
    foreach ($ndk in $carpetasNdkOrdenadas) {
        $version = Split-Path $ndk -Leaf
        $esActivo = ($ndkActivo -and $ndk.TrimEnd('\') -ieq $ndkActivo)
        $lineasNdk.Add("  NDK $version$(if ($esActivo) { '   <-- EN USO' } else { '' })")
        $lineasNdk.Add("    carpeta : $ndk")
        $prebuilt = Join-Path $ndk "toolchains\llvm\prebuilt"
        if (-not (Test-Path -LiteralPath $prebuilt)) {
            $lineasNdk.Add("    (sin toolchains\llvm\prebuilt: el NDK esta incompleto o es de otra plataforma)")
            $lineasNdk.Add("")
            continue
        }
        foreach ($hostDir in (Get-ChildItem -LiteralPath $prebuilt -Directory -ErrorAction SilentlyContinue)) {
            $bin = Join-Path $hostDir.FullName "bin"
            if (-not (Test-Path -LiteralPath $bin)) {
                $lineasNdk.Add("    host    : $($hostDir.Name)  (sin subcarpeta bin)")
                continue
            }
            $nDirsBin++
            $lineasNdk.Add("    host    : $($hostDir.Name)")
            $lineasNdk.Add("    bin     : $bin")
            $lineasNdk.Add("    ESTA EN EL PATH: $(if ($entradasPath -contains $bin.TrimEnd('\')) { 'SI' } else { 'NO' })")
            if ($entradasPath -contains $bin.TrimEnd('\')) {
                $nEnPath++
                continue
            }
            # No esta en el PATH. Solo es un problema si es el NDK en uso: tener
            # NDKs antiguos sin su toolchain en el PATH es lo normal.
            if (-not $esActivo) {
                $lineasNdk.Add("      (no pasa nada: este NDK no es el que se usa. Sialguna vez lo")
                $lineasNdk.Add("       actives con ANDROID_NDK_HOME, anade la de arriba o la de abajo.)")
                $lineasNdk.Add("")
                continue
            }
            $faltan += $bin
            $faltanCrit += $bin
            $lineasNdk.Add("      -> FALTA en el PATH y es el NDK EN USO: Android NO va a compilar.")
            $lineasNdk.Add("         Anadela al PATH de USUARIO con este snippet:")
            $lineasNdk.Add("")
            $lineasNdk.Add("         `$bin  = '$bin'")
            $lineasNdk.Add("         `$k    = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', `$true)")
            $lineasNdk.Add("         `$v    = `$k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)")
            $lineasNdk.Add("         if ((`$v -split ';') -notcontains `$bin) {")
            $lineasNdk.Add("             `$k.SetValue('Path', (`$v.TrimEnd(';') + ';' + `$bin), [Microsoft.Win32.RegistryValueKind]::ExpandString)")
            $lineasNdk.Add("         }")
            $lineasNdk.Add("         `$k.Close()")
            $lineasNdk.Add("")
            $lineasNdk.Add("         OJO: NO uses [Environment]::SetEnvironmentVariable('Path', ...).")
            $lineasNdk.Add("         Ese metodo escribe REG_SZ siempre, y al convertir el PATH de")
            $lineasNdk.Add("         REG_EXPAND_SZ a REG_SZ cualquier %VAR% que se anada despues")
            $lineasNdk.Add("         se queda como texto literal. El snippet de arriba mantiene el tipo.")
            $lineasNdk.Add("")
            $lineasNdk.Add("         Despues cierra sesion y vuelve a entrar.")
        }
        $lineasNdk.Add("")
    }
}

$lineasNdk.Add("SI AUN FALLA EL ENLACE (error 'linker not found' / 'cargo build' sale con codigo 101)")
$lineasNdk.Add("-" * 74)
$lineasNdk.Add("Comprueba que el target esta instalado:")
$lineasNdk.Add("  rustup target add aarch64-linux-android armeabi-v7a-linux-androideabi x86_64-linux-android")
$lineasNdk.Add("")
$lineasNdk.Add("'aarch64-linux-android' NO tiene linker por defecto en rustc: cae a 'cc', que")
$lineasNdk.Add("en Windows normalmente no existe. Por eso hay que indicar cual usar:")
$lineasNdk.Add("")
$lineasNdk.Add("  [Environment]::SetEnvironmentVariable('CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER','<bin>\aarch64-linux-android<API>-clang.cmd','User')")
$lineasNdk.Add("  [Environment]::SetEnvironmentVariable('CC_aarch64-linux-android',                  '<bin>\aarch64-linux-android<API>-clang.cmd','User')")
$lineasNdk.Add("  [Environment]::SetEnvironmentVariable('AR_aarch64-linux-android',                  '<bin>\llvm-ar.exe','User')")
$lineasNdk.Add("")
$lineasNdk.Add("  <API> = la minima version de Android que soporta tu proyecto (Tauri v2 usa")
$lineasNdk.Add("  24 por defecto). El wrapper debe existir en la carpeta bin: comprueba con")
$lineasNdk.Add("  dir <bin>\aarch64-linux-android*-clang.cmd")
$lineasNdk.Add("")
$lineasNdk.Add("Aqui NO hace falta cuidar el tipo del registro: estas variables no usan %VAR%,")
$lineasNdk.Add("asi que REG_SZ esta bien (a diferencia del PATH, que si debe ser ExpandString).")
$lineasNdk.Add("")
$lineasNdk.Add("Recordatorio: la entrada del PATH lleva la VERSION del NDK a mano. Si")
$lineasNdk.Add("actualizas el NDK, esa entrada deja de apuntar a nada y hay que")
$lineasNdk.Add("reemplazarla por la de la version nueva.")
$lineasNdk.Add("")

Save-Texto ($lineasNdk -join "`r`n") (Join-Path $dirInventario "android-ndk-toolchain.txt")
if ($nDirsBin -eq 0) {
    Add-Resultado "android-ndk-toolchain.txt" "OMITIDO" "no hay ningun NDK instalado"
} elseif ($faltanCrit.Count -gt 0) {
    Add-Resultado "android-ndk-toolchain.txt" "AVISO" "el NDK EN USO ($(Split-Path $ndkActivo -Leaf)) no tiene su toolchain en el PATH - Android NO compilara hasta anadirla"
} else {
    $detalle = "$nDirsBin NDK instalado(s), el que esta en uso tiene su toolchain en el PATH"
    if ($faltan.Count -gt 0) { $detalle += ". $($faltan.Count) NDK antiguo(s) sin toolchain en el PATH (normal, no afecta)" }
    Add-Resultado "android-ndk-toolchain.txt" "OK" $detalle
}

# --- Rust ---
if (Test-Comando "rustup") {
    Save-Resultado "rustup show"     "rustup" @('show')                        (Join-Path $dirInventario "rustup-show.txt")
    Save-Resultado "rustup toolchain" "rustup" @('toolchain', 'list')           (Join-Path $dirInventario "rustup-toolchains.txt")
    Save-Resultado "rustup component" "rustup" @('component', 'list')           (Join-Path $dirInventario "rustup-componentes.txt")
    Save-Resultado "rustup target"    "rustup" @('target', 'list')              (Join-Path $dirInventario "rustup-targets.txt")
} else {
    Add-Resultado "rustup" "OMITIDO" "rustup no esta instalado"
}
if (Test-Comando "cargo") {
    Save-Resultado "cargo install" "cargo" @('install', '--list') (Join-Path $dirInventario "cargo-installados.txt")
}

# --- Visual Studio ---
$vsw = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path -LiteralPath $vsw) {
    Save-Resultado "Visual Studio" $vsw @('-products', '*', '-format', 'json', '-utf8') (Join-Path $dirInventario "visual-studio.json")
    Save-Resultado "VS workloads"   $vsw @('-products', '*', '-format', 'json', '-utf8', '-prerelease', '-requires', 'Microsoft.VisualStudio.Workload.*') (Join-Path $dirInventario "visual-studio-workloads.json")
    Add-Resultado "Visual Studio" "OK" "vswhere.json (instalaciones y workloads)"
} else {
    Add-Resultado "Visual Studio" "OMITIDO" "vswhere.exe no encontrado (no hay VS instalado, o es VS 2022 sin Installer)"
}

# --- VS Code / Cursor ---
function Save-Extensiones {
    param([string]$Area, [string]$Comando, [string]$NombreArchivo)
    $exe = Resolve-Comando $Comando
    if (-not $exe) { Add-Resultado $Area "OMITIDO" "$Comando no esta en el PATH"; return }
    $dest = Join-Path $dirInventario $NombreArchivo
    $r = Invoke-Native -FilePath $exe -Arguments @('--list-extensions', '--show-versions') -OutFile $dest
    if ($r.Ok) {
        $n = (Get-Content -LiteralPath $dest -ErrorAction SilentlyContinue | Measure-Object).Count
        Add-Resultado $Area "OK" "$n extension(es) - $NombreArchivo"
    } else {
        Add-Resultado $Area "AVISO" "no se pudieron listar las extensiones (exit $($r.ExitCode))"
    }
}
Save-Extensiones "VS Code"  "code"   "vscode-extensiones.txt"
Save-Extensiones "Cursor"   "cursor" "cursor-extensiones.txt"

# --- Node / npm ---
if (Test-Comando "npm") {
    Save-Resultado "npm global" "npm" @('ls', '-g', '--depth=0') (Join-Path $dirInventario "npm-global.txt")
    Save-Resultado "npm config" "npm" @('config', 'list')      (Join-Path $dirInventario "npm-config.txt")
}

# --- Python ---
$py = (Get-Command python -ErrorAction SilentlyContinue)
if ($py) {
    Save-Resultado "pip" $py.Source @('-m', 'pip', 'freeze') (Join-Path $dirInventario "pip-freeze.txt")
}
if (Test-Comando "py") { Save-Resultado "py -0p" "py" @('-0p') (Join-Path $dirInventario "python-instalados.txt") }

# --- .NET ---
if (Test-Comando "dotnet") {
    Save-Resultado "dotnet sdks" "dotnet" @('--list-sdks')  (Join-Path $dirInventario "dotnet-sdks.txt")
    Save-Resultado "dotnet runtimes" "dotnet" @('--list-runtimes') (Join-Path $dirInventario "dotnet-runtimes.txt")
    Save-Resultado "dotnet tools" "dotnet" @('tool', 'list', '-g')  (Join-Path $dirInventario "dotnet-tools.txt")
}

# --- Git ---
if (Test-Comando "git") {
    Save-Resultado "git config" "git" @('config', '--list', '--show-origin') (Join-Path $dirInventario "git-config.txt")
}

# --- Chocolatey ---
if (Test-Comando "choco") {
    $dest = Join-Path $dirInventario "chocolatey-paquetes.txt"
    $r = Invoke-Native -FilePath "choco" -Arguments @('list', '--local-only', '--limit-output') -OutFile $dest
    if (-not $r.Ok) {
        $r = Invoke-Native -FilePath "choco" -Arguments @('list', '-lo') -OutFile $dest
    }
    if ($r.Ok) { Add-Resultado "Chocolatey" "OK" "chocolatey-paquetes.txt" }
    else { Add-Resultado "Chocolatey" "AVISO" "choco list fallo (exit $($r.ExitCode))" }
}

# --- Winget ---
if (Test-Comando "winget") {
    $dest = Join-Path $dirInventario "apps-winget.json"
    $oldEap = $ErrorActionPreference; $ErrorActionPreference = "Continue"
    try { & winget.exe export -o "$dest" --accept-source-agreements --disable-interactivity 2>&1 | Out-Null }
    finally { $ErrorActionPreference = $oldEap }
    if (Test-Path -LiteralPath $dest) {
        Add-Resultado "Winget" "OK" ("{0} - apps-winget.json" -f (Get-FormatoBytes (Get-Item -LiteralPath $dest).Length))
    } else {
        Add-Resultado "Winget" "AVISO" "winget export no genero archivo"
    }
} else {
    Add-Resultado "Winget" "OMITIDO" "winget no instalado"
}

# --- Fuentes instaladas ---
try {
    $fuentes = (Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts").PSObject.Properties |
               Where-Object { $_.Name -notlike "PS*" } | Select-Object -ExpandProperty Name
    Save-Texto ($fuentes -join "`r`n") (Join-Path $dirInventario "fuentes-instaladas.txt")
    Add-Resultado "Fuentes" "OK" "$($fuentes.Count) fuente(s)"
} catch { Add-Resultado "Fuentes" "AVISO" $_.Exception.Message }

# ============================================================================
#  4. ARCHIVOS DE CONFIGURACION (lo que no se puede regenerar solo)
# ============================================================================

Write-Titulo "4. Archivos de configuracion"

if (-not $SoloInventario) {
    $u = $env:USERPROFILE
    $ap = $env:APPDATA
    $lo = $env:LOCALAPPDATA

    # --- Git ---
    Copy-Archivo "git .gitconfig"      (Join-Path $u ".gitconfig")      "git\.gitconfig"          $dirArchivos
    Copy-Archivo "git .gitattributes"  (Join-Path $u ".gitattributes")  "git\.gitattributes"      $dirArchivos
    if (Test-Path -LiteralPath (Join-Path $u ".config\git")) {
        Copy-Arbol "git config extra" (Join-Path $u ".config\git") "git\config" $dirArchivos
    }

    # --- SSH (contiene claves privadas) ---
    if (Test-Path -LiteralPath (Join-Path $u ".ssh")) {
        Copy-Arbol "claves SSH" (Join-Path $u ".ssh") "ssh" $dirArchivos
        Add-Resultado "" "AVISO" "La carpeta ssh contiene claves PRIVADAS: guardala cifrada y con cuidado"
    }

    # --- VS Code / Cursor: solo configuracion, nunca las caches (cientos de MB) ---
    $excluirEditor = @("workspaceStorage", "globalStorage", "History", "Backups", "logs", "Cache", "CachedData", "CachedExtensions", "node_modules", "User\workspaceStorage")
    Copy-Arbol "VS Code config" (Join-Path $ap "Code\User")   "vscode\User"   $dirArchivos -ExcluirDir $excluirEditor
    Copy-Arbol "Cursor config"  (Join-Path $ap "Cursor\User") "cursor\User"   $dirArchivos -ExcluirDir $excluirEditor

    # --- Windows Terminal ---
    $wt = Get-ChildItem -Path (Join-Path $lo "Packages") -Directory -Filter "Microsoft.WindowsTerminal*" -ErrorAction SilentlyContinue
    foreach ($w in $wt) {
        Copy-Archivo "Windows Terminal" (Join-Path $w.FullName "LocalState\settings.json") ("terminal\$($w.Name)\settings.json") $dirArchivos
        Copy-Archivo "Windows Terminal (perfiles)" (Join-Path $w.FullName "LocalState\state.json") ("terminal\$($w.Name)\state.json") $dirArchivos
    }

    # --- Perfiles de PowerShell ---
    Copy-Arbol "Perfil PowerShell" (Join-Path $u "Documents\PowerShell") "powershell\PowerShell" $dirArchivos
    Copy-Arbol "Perfil PS clasico" (Join-Path $u "Documents\WindowsPowerShell") "powershell\WindowsPowerShell" $dirArchivos

    # --- Android: licencias, keystore, AVDs ---
    if ($androidRoot -and (Test-Path -LiteralPath (Join-Path $androidRoot "licenses"))) {
        Copy-Arbol "Android licencias" (Join-Path $androidRoot "licenses") "android\licenses" $dirArchivos
    }
    $dotAndroid = Join-Path $u ".android"
    Copy-Archivo "debug.keystore" (Join-Path $dotAndroid "debug.keystore") "android\debug.keystore" $dirArchivos
    foreach ($ini in @("adbkey", "adbkey.pub")) {
        Copy-Archivo "Android $ini" (Join-Path $dotAndroid $ini) "android\$ini" $dirArchivos
    }
    $avdDir = Join-Path $dotAndroid "avd"
    if (Test-Path -LiteralPath $avdDir) {
        # Configuracion de los emuladores siempre; las imagenes (.avd) solo si se piden
        $nombresAvd = Get-ChildItem -LiteralPath $avdDir -Directory -Filter "*.avd" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty BaseName
        if ($ConImagenesEmulador) {
            Copy-Arbol "Android AVDs (completos)" $avdDir "android\avd" $dirArchivos
        } else {
            foreach ($ini in Get-ChildItem -LiteralPath $avdDir -Filter "*.ini" -File -ErrorAction SilentlyContinue) {
                Copy-Archivo "AVD config" $ini.FullName "android\avd-config\$($ini.Name)" $dirArchivos
            }
            foreach ($n in $nombresAvd) {
                $cfg = Join-Path $avdDir "$n.avd\config.ini"
                Copy-Archivo "AVD $n" $cfg "android\avd-config\$n.ini" $dirArchivos
            }
            Add-Resultado "AVDs" "AVISO" "$($nombresAvd.Count) emulador(es) - solo configuracion. Usa -ConImagenesEmulador para copiar las imagenes"
        }
    }

    # --- Gradle / Maven ---
    foreach ($f in @("gradle.properties", "init.gradle", "init.gradle.kts")) {
        Copy-Archivo "Gradle $f" (Join-Path $gradleHome $f) "gradle\$f" $dirArchivos
    }
    if (Test-Path -LiteralPath (Join-Path $gradleHome "init.d")) {
        Copy-Arbol "Gradle init.d" (Join-Path $gradleHome "init.d") "gradle\init.d" $dirArchivos
    }
    $m2 = Join-Path $u ".m2"
    Copy-Archivo "Maven settings.xml"    (Join-Path $m2 "settings.xml")      "maven\settings.xml"      $dirArchivos
    Copy-Archivo "Maven gradle.properties" (Join-Path $m2 "gradle.properties") "maven\gradle.properties" $dirArchivos
    if (Test-Path -LiteralPath (Join-Path $m2 "settings-security.xml")) {
        Copy-Archivo "Maven settings-security" (Join-Path $m2 "settings-security.xml") "maven\settings-security.xml" $dirArchivos
        Add-Resultado "" "AVISO" "settings-security.xml contiene contrasenas cifradas: normalmente hay que reintroducirlas"
    }

    # --- Rust ---
    foreach ($f in @("config.toml", "config")) {
        Copy-Archivo "Cargo $f" (Join-Path $u ".cargo\$f") "cargo\$f" $dirArchivos
    }
    if (Test-Path -LiteralPath (Join-Path $u ".rustup\settings.toml")) {
        Copy-Archivo "rustup settings" (Join-Path $u ".rustup\settings.toml") "cargo\rustup-settings.toml" $dirArchivos
    }

    # --- Flutter / Dart ---
    foreach ($p in @((Join-Path $u ".flutter_settings"), (Join-Path $ap "flutter\settings"))) {
        if (Test-Path -LiteralPath $p) { Copy-Archivo "Flutter settings" $p "flutter\settings.json" $dirArchivos; break }
    }
    $pubCache = [Environment]::GetEnvironmentVariable("PUB_CACHE", "User")
    if ([string]::IsNullOrWhiteSpace($pubCache)) { $pubCache = Join-Path $lo "Pub\Cache" }
    $lineasPub = @("PUB_CACHE = $pubCache")
    if (Test-Path -LiteralPath (Join-Path $pubCache "hosted")) {
        $lineasPub += "Paquetes descargados:"
        foreach ($hostDir in Get-ChildItem -LiteralPath (Join-Path $pubCache "hosted") -Directory -ErrorAction SilentlyContinue) {
            $pkgs = Get-ChildItem -LiteralPath $hostDir.FullName -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name
            $lineasPub += "  $($hostDir.Name): $($pkgs.Count) paquetes"
        }
    }
    Save-Texto ($lineasPub -join "`r`n") (Join-Path $dirInventario "pub-cache.txt")
    Add-Resultado "pub-cache.txt" "OK" "paquetes Dart/Flutter en cache (se redescargan solos)"

    # --- Node / Python / NuGet ---
    Copy-Archivo ".npmrc"    (Join-Path $u ".npmrc")        "node\.npmrc"     $dirArchivos
    Copy-Archivo ".condarc"  (Join-Path $u ".condarc")      "python\.condarc" $dirArchivos
    Copy-Archivo "pip.ini"   (Join-Path $ap "pip\pip.ini")  "python\pip.ini"  $dirArchivos
    Copy-Archivo "NuGet.Config" (Join-Path $ap "NuGet\NuGet.Config") "dotnet\NuGet.Config" $dirArchivos

    # --- Sistema ---
    Copy-Archivo "hosts" (Join-Path $env:SystemRoot "System32\drivers\etc\hosts") "sistema\hosts" $dirArchivos
} else {
    Add-Resultado "archivos de configuracion" "OMITIDO" "modo -SoloInventario"
}

# ============================================================================
#  5. LIMPIEZA DE RESPALDOS ANTERIORES
# ============================================================================

Write-Titulo "5. Limpieza de respaldos anteriores"

# OJO con -SoloInventario: ese respaldo NO lleva la carpeta 'archivos' (claves SSH,
# configs de editores, keystore...). Si podara aqui, borraria el unico respaldo
# completo que puede haber. Por eso en ese modo la limpieza se salta siempre.
if ($SoloInventario) {
    Add-Resultado "respaldos anteriores" "OK" "limpieza OMITIDA: en modo -SoloInventario este respaldo no tiene 'archivos' y podria destruir el unico respaldo completo"
    Write-Host ""
    Write-Host "  [AVISO] No se borran respaldos anteriores: este es un respaldo de solo" -ForegroundColor Yellow
    Write-Host "          inventario (sin claves SSH ni configuraciones). Asi no se" -ForegroundColor Yellow
    Write-Host "          destruye el unico respaldo completo que tengas." -ForegroundColor Yellow
} else {

# Se hace aqui, y NO antes de crear el respaldo nuevo, a proposito: si el backup
# falla a mitad todavia tienes el anterior intacto. Ademas, borrar aqui permite
# que el aviso de esta limpieza quede registrado en inventario.json.

$respaldosViejos = @()
if (Test-Path -LiteralPath $Destino -PathType Container) {
    # El nombre lleva timestamp yyyy-MM-dd_HHmmss, asi que ordenar por nombre
    # descendente es ordenar por fecha. Aun asi se excluye explcitamente la
    # carpeta en la que estamos escribiendo por si el reloj del sistema cambiase.
    $todos = @(Get-ChildItem -LiteralPath $Destino -Directory -Filter "EnvBackup_*" -ErrorAction SilentlyContinue |
               Where-Object { $_.FullName -ne $carpetaOut } |
               Sort-Object Name -Descending)
    $respaldosViejos = @($todos | Select-Object -Skip ($ConservarN - 1))
}

if ($respaldosViejos.Count -eq 0) {
    Add-Resultado "respaldos anteriores" "OK" "se conservan $ConservarN (no hay nada que borrar)"
} else {
    Write-Host ""
    Write-Host "  Se conservaran $ConservarN respaldo(s). Se van a BORRAR $($respaldosViejos.Count):" -ForegroundColor Yellow
    $borrados = 0
    $fallosBorrado = @()
    foreach ($viejo in $respaldosViejos) {
        # Un respaldo restaurado contiene pre-restore_* con los .reg de rollback.
        # Son datos de seguridad que no se pueden regenerar: avisa antes de perderlos.
        $rollbacks = @(Get-ChildItem -LiteralPath $viejo.FullName -Directory -Filter "pre-restore_*" -ErrorAction SilentlyContinue)
        if ($rollbacks.Count -gt 0) {
            Write-Host ("    [AVISO] {0} tiene {1} carpeta(s) pre-restore_* con datos de rollback de una restauracion previa." -f $viejo.Name, $rollbacks.Count) -ForegroundColor Yellow
            Write-Host "             Se borran con el respaldo. Copialas fuera si todavia las necesitas." -ForegroundColor Yellow
            if (-not $SinConfirmacion) {
                $ok = Read-Host "             Continuar y borrarlo? (s/N)"
                if ($ok -ne "s" -and $ok -ne "S") {
                    Write-Host "             Conservado." -ForegroundColor DarkGray
                    continue
                }
            }
        }
        try {
            # Se mide ANTES de borrar, si no el conteo siempre sale en 0.
            $m = Get-ChildItem -LiteralPath $viejo.FullName -Recurse -File -ErrorAction SilentlyContinue |
                 Measure-Object -Property Length -Sum
            $tamTxt = if ($m.Count -gt 0) { "$($m.Count) archivos, $(Get-FormatoBytes $m.Sum)" } else { "vacio" }
            Remove-Item -LiteralPath $viejo.FullName -Recurse -Force -ErrorAction Stop
            Write-Host ("    [OK]    {0} borrado ({1})" -f $viejo.Name, $tamTxt) -ForegroundColor DarkGreen
            $borrados++
        } catch {
            # No es un fallo del backup: el respaldo nuevo esta bien. Solo no se
            # pudo limpiar. Se avisa como AVISO para no marcar el backup como erroneo.
            Write-Host ("    [AVISO]  {0} no se pudo borrar: {1}" -f $viejo.Name, $_.Exception.Message) -ForegroundColor Yellow
            $fallosBorrado += $viejo.Name
        }
    }
    $detalleBorrado = "$borrados borrado(s) de $($respaldosViejos.Count), se conservan $ConservarN"
    if ($fallosBorrado.Count -gt 0) { $detalleBorrado += ". No se pudieron borrar: $($fallosBorrado -join ', ')" }
    Add-Resultado "respaldos anteriores" "OK" $detalleBorrado

}   # fin del if/else de $respaldosViejos

}   # fin del else de -SoloInventario

# ============================================================================
#  6. MANIFIESTO Y GUIA DE RESTAURACION
# ============================================================================

Write-Titulo "6. Manifiesto"

$inventario = [ordered]@{
    version            = 2
    fecha              = $script:Inicio.ToString("o")
    equipo             = $env:COMPUTERNAME
    usuario            = $env:USERNAME
    usuarioSID         = (whoami /user /fo csv /nh 2>$null)
    sistemaOperativo   = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).Caption
    versionSO          = [string][System.Environment]::OSVersion.Version
    esAdmin            = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    soloInventario     = [bool]$SoloInventario
    resultados         = @($script:Resultados | ForEach-Object { [ordered]@{ area = $_.Area; estado = $_.Estado; detalle = $_.Detalle } })
    variablesUsuario   = @(Get-Item -LiteralPath "HKCU:\Environment" | ForEach-Object {
                            $k = $_
                            $k.GetValueNames() | ForEach-Object { [ordered]@{ nombre = $_; valor = $k.GetValue($_) } } })
    variablesSistema   = @(Get-Item -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment" | ForEach-Object {
                            $k = $_
                            $k.GetValueNames() | ForEach-Object { [ordered]@{ nombre = $_; valor = $k.GetValue($_, "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } } })
}

$inventario | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $carpetaOut "inventario.json") -Encoding utf8
Add-Resultado "inventario.json" "OK" "manifiesto con variables, entorno y resultados"

# Guia de restauracion generada a partir de lo que realmente se guardo
$lineas = New-Object System.Collections.Generic.List[string]
$lineas.Add("COMO RESTAURAR ESTE BACKUP")
$lineas.Add("=" * 60)
$lineas.Add("Backup generado : $($script:Inicio)")
$lineas.Add("Equipo          : $env:COMPUTERNAME")
$lineas.Add("Usuario         : $env:USERNAME")
$lineas.Add("")
$lineas.Add("PASO 1 - Variables de entorno (automatico)")
$lineas.Add("  cd '$carpetaOut'")
$lineas.Add("  ..\Restore-Variables.ps1 -CarpetaBackup '$carpetaOut'")
$lineas.Add("  El script pide confirmacion, crea un punto de restauracion, hace un")
$lineas.Add("  respaldo del estado actual, adapta las rutas C:\Users\<nombre> si el")
$lineas.Add("  usuario cambio, importa los .reg y verifica variable por variable.")
$lineas.Add("  DESPUES DE RESTAURAR: cierra sesion y vuelve a entrar (o reinicia).")
$lineas.Add("  Si solo cierras las terminales puede que veas los valores antiguos.")
$lineas.Add("")
$lineas.Add("PASO 2 - Reinstalar aplicaciones")
if (Test-Path -LiteralPath (Join-Path $dirInventario "apps-winget.json")) {
    $lineas.Add("  winget import --import-file 'inventario\apps-winget.json'")
} else {
    $lineas.Add("  (no se guardo lista de winget)")
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "chocolatey-paquetes.txt")) {
    $lineas.Add("  choco install -y <paquete>   # ver inventario\chocolatey-paquetes.txt")
}
$lineas.Add("  Extensiones de editores:  code --install-extensions <id>   (ver vscode-extensiones.txt)")
$lineas.Add("")
$lineas.Add("PASO 3 - Configuraciones en 'archivos\''")
$lineas.Add("  Copia manual de lo que necesites, por ejemplo:")
$lineas.Add("    copy .\archivos\git\.gitconfig  %USERPROFILE%\")
$lineas.Add("    copy .\archivos\vscode\User\settings.json  %APPDATA%\Code\User\")
$lineas.Add("  (Restore-Variables.ps1 -RestaurarArchivos lo hace automaticamente si lo pides)")
$lineas.Add("")
$lineas.Add("PASO 4 - SDKs y toolchains (se reinstalan, no se respaldan en binario)")
$lineas.Add("  Java   : reinstala el JDK de la version guardada en jdk-instalados.txt")
$lineas.Add("           y ajusta JAVA_HOME.")
$lineas.Add("  Flutter: reinstala Flutter (misma version en flutter-version.txt) y luego")
$lineas.Add("           'flutter doctor' y acepta las licencias.")
$lineas.Add("  Android: reinstala el Android Studio / cmdline-tools, luego en E:\SDKs\Android:")
$lineas.Add("           sdkmanager --sdk_root=E:\SDKs\Android <paquetes de android-paquetes-instalados.txt>")
$lineas.Add("           Las licencias estan copiadas en .\archivos\android\licenses")
$lineas.Add("           Copia .\archivos\android\debug.keystore a %USERPROFILE%\.android\")
$lineas.Add("  NDK    : es OBLIGATORIO para compilar Rust/Tauri en Android. El .reg ya")
$lineas.Add("           restaura ANDROID_NDK_HOME y la entrada del PATH, pero revisa")
$lineas.Add("           .\inventario\android-ndk-toolchain.txt: ahi queda escrito que")
$lineas.Add("           carpeta \\toolchains\\llvm\\prebuilt\\<host>\\bin debes anadir al")
$lineas.Add("           PATH y si ya esta dentro. Sin ella, 'tauri android build'")
$lineas.Add("           falla con 'cargo build' codigo 101.")
$lineas.Add("  Rust   : rustup toolchain install <los de rustup-toolchains.txt>")
$lineas.Add("           rustup target add aarch64-linux-android armeabi-v7a-linux-androideabi x86_64-linux-android")
$lineas.Add("           cargo install --list  -> reinstala los binarios de cargo-instalados.txt")
$lineas.Add("  Gradle : revisa inventario\gradle-estado.txt (distribuciones y cache)")
$lineas.Add("  VS     : revisa inventario\visual-studio.json para replicar workloads")
$lineas.Add("")
$lineas.Add("AVISO")
$lineas.Add("  La carpeta 'ssh' contiene claves privadas y 'maven' puede contener")
$lineas.Add("  contrasenas. Si guardas este backup en la nube, cifralo primero.")
$lineas.Add("")
Save-Texto ($lineas -join "`r`n") (Join-Path $carpetaOut "COMO-RESTAURAR.txt")
Add-Resultado "COMO-RESTAURAR.txt" "OK" "guia de restauracion"

# ============================================================================
#  RESUMEN  (siempre se imprime, nunca dentro de un try/if)
# ============================================================================

$fin = Get-Date
$ok     = @($script:Resultados | Where-Object { $_.Estado -eq "OK" })
$avisos = @($script:Resultados | Where-Object { $_.Estado -eq "AVISO" })
$fallos = @($script:Resultados | Where-Object { $_.Estado -eq "FALLO" })

Write-Host ""
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  RESUMEN" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  Correctos : $($ok.Count)"
Write-Host "  Avisos    : $($avisos.Count)" -ForegroundColor $(if ($avisos.Count) { "Yellow" } else { "Gray" })
Write-Host "  Fallos    : $($fallos.Count)" -ForegroundColor $(if ($fallos.Count) { "Red" } else { "Gray" })
Write-Host "  Duracion  : $([int]($fin - $script:Inicio).TotalSeconds) s"

if ($avisos.Count -gt 0) {
    Write-Host ""
    Write-Host "  AVISOS:" -ForegroundColor Yellow
    $avisos | ForEach-Object { Write-Host ("    - {0}: {1}" -f $_.Area, $_.Detalle) -ForegroundColor Yellow }
}
if ($fallos.Count -gt 0) {
    Write-Host ""
    Write-Host "  FALLOS:" -ForegroundColor Red
    $fallos | ForEach-Object { Write-Host ("    - {0}: {1}" -f $_.Area, $_.Detalle) -ForegroundColor Red }
}

$tamTotal = (Get-ChildItem -LiteralPath $carpetaOut -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Sum Length)
Write-Host ""
Write-Host "  Backup completo en: $carpetaOut"
Write-Host "  Tamano total      : $(Get-FormatoBytes $tamTotal.Sum)"
Write-Host ""
if ($unidadDestino -and (Join-Path $unidadDestino "\") -ieq "C:\") {
    Write-Host "  IMPORTANTE: esta en C:. Copia esta carpeta a otra unidad antes de formatear." -ForegroundColor Yellow
}
# Aviso real, no generico: si no habia .ssh no hay nada que proteger, y decirlo
# en falso hace que el aviso deje de heederse.
if (Test-Path -LiteralPath (Join-Path $env:USERPROFILE ".ssh")) {
    Write-Host "  IMPORTANTE: no borres esta carpeta. Contiene claves SSH privadas." -ForegroundColor Yellow
}
Write-Host ""
Write-Host "  Siguiente paso: .\Restore-Variables.ps1 -CarpetaBackup `"$carpetaOut`""
Write-Host ""

if ($fallos.Count -gt 0) { exit 1 }
exit 0