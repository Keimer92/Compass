#Requires -Version 5.1
<#
    Restore-Variables.ps1
    Restaura el entorno de desarrollo desde un backup generado por Backup-Variables.ps1.

    Lo que hace:
      1. Valida el backup y avisa de unidades de disco que ya no existan.
      2. Muestra el diff entre el backup y el estado actual (sin tocar nada si usas -SoloValidar).
      3. Crea un punto de restauracion del sistema (si eres administrador).
      4. Guarda el estado ACTUAL de las variables antes de cambiar nada.
      5. Reescribe las rutas C:\Users\<nombre> si el usuario de Windows cambio.
      6. Importa los .reg y comprueba variable por variable que el valor quedo puesto.
      7. Emite WM_SETTINGCHANGE para que las aplicacionesandezn los valores nuevos.

    Uso:
      .\Restore-Variables.ps1 -CarpetaBackup "E:\Backups\EnvBackup_2026-08-29_120000"
      .\Restore-Variables.ps1                              # busca los backups y pregunta
      .\Restore-Variables.ps1 -SoloValidar                 # solo informa, no cambia nada
      .\Restore-Variables.ps1 -Force -UsuarioNuevo "Juan"   # sin preguntas, indicando el usuario
      .\Restore-Variables.ps1 -RestaurarArchivos           # ademas recupera git/ssh/vscode/...
#>
[CmdletBinding()]
param(
    [string]$CarpetaBackup = "",
    [switch]$Force,
    [switch]$SoloValidar,
    [switch]$NoAdaptarUsuario,
[string]$UsuarioNuevo = "",
    [string[]]$NombresExtra = @(),
    [switch]$RestaurarArchivos,
    [switch]$OmitirPuntoRestauracion
)

$ErrorActionPreference = "Stop"
$script:Inicio = Get-Date

# Nombres de carpeta de perfil que NO son el usuario real
$PerfilesDelSistema = @("Public", "Default", "Default User", "All Users", "DefaultAppPool", "WDAGUtilityAccount", "SYSTEM")

# ============================================================================
#  HELPERS
# ============================================================================

function Write-Titulo { param([string]$T) Write-Host ""; Write-Host "--- $T ---" -ForegroundColor Cyan }

function Get-RegEntorno {
    # Lee un .reg de clave Environment. Maneja continuaciones de linea con '\',
    # valores dword: y hex(2):, y devuelve los valores SIN expandir.
    param([string]$RutaReg)

    if (-not $RutaReg -or -not (Test-Path -LiteralPath $RutaReg)) { return $null }

    $mapa     = [ordered]@{}
    $enClave  = $false
    $buffer   = ""

    foreach ($linea in (Get-Content -LiteralPath $RutaReg)) {
        $t = $linea.Trim()

        if ($buffer -eq "") {
            if ($t.StartsWith("[")) {
                $enClave = $t -match '\\Environment\]$'
                continue
            }
            if (-not $enClave) { continue }
            if ($t -eq "" -or $t.StartsWith("@") -or $t.StartsWith('"-"')) { continue }
            $buffer = $t
        } else {
            $buffer += $t
        }

        if ($buffer.EndsWith("\")) { $buffer = $buffer.Substring(0, $buffer.Length - 1); continue }

        $registro = $buffer
        $buffer   = ""

        if ($registro -notmatch '^"(?<k>[^"]+)"=(?<v>.*)$') { continue }
        $nombre = $Matches["k"]
        $crudo  = $Matches["v"]

        if ($crudo -match '^dword:(?<n>\d+)$') {
            $mapa[$nombre] = @{ Tipo = "REG_DWORD"; Valor = [string][uint32]$Matches["n"] }
        } elseif ($crudo -match '^hex\((?<t>[^)]*)\):(?<d>.*)$') {
            $tipoHex = $Matches["t"]
            $datosHex = $Matches["d"]
            if ($tipoHex -eq "2") {
                # REG_EXPAND_SZ exportada como hexadecimal: reconstruir el texto
                $bytes = @()
                if ($datosHex) {
                    $bytes = @($datosHex -split "," | Where-Object { $_ -match '^\s*[0-9a-fA-F]{2}\s*$' } | ForEach-Object { [Convert]::ToByte($_.Trim(), 16) })
                }
                $texto = ""
                if ($bytes.Count -gt 0) { $texto = [System.Text.Encoding]::Unicode.GetString([byte[]]$bytes).TrimEnd([char]0) }
                $mapa[$nombre] = @{ Tipo = "REG_EXPAND_SZ"; Valor = $texto }
            } else {
                $mapa[$nombre] = @{ Tipo = "REG_BINARY"; Valor = "" }
            }
        } else {
            $mapa[$nombre] = @{ Tipo = "REG_SZ"; Valor = $crudo.Trim('"').Replace('\\', '\') }
        }
    }
    return $mapa
}

function Get-UsuariosDetectados {
    # Nombres de CARPETA DE PERFIL, deducidos solo del patron "\Users\<nombre>\".
    # Es la unica fuente fiable: en los .reg, "C:\Users\Juan", "D:\Users\Juan" y
    # "\\servidor\Users\Juan" son la misma convencion de Windows.
    #
    # IMPORTANTE: NO se deducen nombres del primer segmento tras la unidad
    # (C:\Proyectos, C:\Program Files, D:\Juan...). Ese patron es demasiado ambiguo:
    # daria nombres de usuario falsa como "Users" o "Proyectos" y destrozaria las rutas.
    param([string[]]$RutasReg)
    $usuarios = @()
    foreach ($r in $RutasReg) {
        if (-not $r -or -not (Test-Path -LiteralPath $r)) { continue }
        $contenido = Get-Content -LiteralPath $r -Raw
        #  "\Users\<nombre>" seguido de barra o de comilla de fin de valor
        foreach ($m in [regex]::Matches($contenido, 'Users\\\\+([^\\"]+?)(?=\\\\|")')) {
            $u = $m.Groups[1].Value
            if ($u -and ($usuarios -notcontains $u)) { $usuarios += $u }
        }
    }
    return $usuarios
}

# Carpetas que jamas son un nombre de usuario aunque aparezcan tras la barra de una unidad.
$CarpetasDelSistema = @(
    "Users", "Perfiles", "Windows", "Program Files", "Program Files (x86)", "ProgramData",
    "WindowsApps", "Temp", "tmp", "System32", "SysWOW64", "Recovery", "boot", "efi",
    "Intel", "PerfLogs", "Drivers", "Games", "Tools", "SDKs", "Apps", "Config", "Data",
    "Documents and Settings", "OneDriveTemp", "Packages", "Common Files", "Sources"
)

# Reescribe <nombre> SOLO cuando ocupa un segmento de ruta completo, es decir, delimitado
# por barras o por el final del valor. Asi:
#   C:\Users\Juan\AppData        -> C:\Users\Admin\AppData       (si, coincide)
#   D:\Users\Juan\proyecto       -> D:\Users\Admin\proyecto      (si, coincide)
#   \\servidor\Juan\proyecto     -> \\servidor\Admin\proyecto    (si, coincide)
#   C:\Users\JuanSantos          -> NO se toca con el nombre "Juan" (no es el mismo segmento)
#   C:\Proyectos\Juan-backups    -> NO se toca (el segmento es "Juan-backups")
function Ajustar-NombreEnReg {
    param([string]$Contenido, [string[]]$NombresViejos, [string]$UsuarioNuevo)
    $salida = $Contenido
    $nombresUsados = New-Object System.Collections.Generic.List[string]
    foreach ($viejo in ($NombresViejos | Sort-Object -Property Length -Descending)) {
        if ($viejo -eq $UsuarioNuevo) { continue }
        $patron = '(?<=[\\"])' + [regex]::Escape($viejo) + '(?=[\\"])'
        $veces = ([regex]::Matches($salida, $patron)).Count
        if ($veces -eq 0) { continue }
        $salida = [regex]::Replace($salida, $patron, $UsuarioNuevo)
        $nombresUsados.Add("$viejo -> $UsuarioNuevo ($veces)")
    }
    return [pscustomobject]@{ Contenido = $salida; Resumen = $nombresUsados }
}

function Get-InformeCambios {
    # Diff real linea a linea entre el .reg original y el ajustado.
    param([string]$Antes, [string]$Despues)
    $la = $Antes   -split "`r?`n"
    $lb = $Despues -split "`r?`n"
    $lineas = New-Object System.Collections.Generic.List[string]
    for ($i = 0; $i -lt [Math]::Min($la.Count, $lb.Count); $i++) {
        if ($la[$i].Trim() -ne $lb[$i].Trim()) {
            $lineas.Add("    - $(Compartir-Texto $la[$i].Trim() 96)")
            $lineas.Add("    + $(Compartir-Texto $lb[$i].Trim() 96)")
        }
    }
    return $lineas
}

function Get-UnidadesUsadas {
    # Todas las unidades X:\ mencionadas en los .reg del backup
    param([string[]]$RutasReg)
    $unidades = @()
    foreach ($r in $RutasReg) {
        if (-not $r -or -not (Test-Path -LiteralPath $r)) { continue }
        $contenido = Get-Content -LiteralPath $r -Raw
        foreach ($m in [regex]::Matches($contenido, '\b([A-Z]):\\')) {
            $u = $m.Groups[1].Value.ToUpper()
            if ($unidades -notcontains $u) { $unidades += $u }
        }
    }
    return ($unidades | Sort-Object)
}

function Send-NotificacionEntorno {
    # WM_SETTINGCHANGE / "Environment": sin esto, las apps seguirian con los valores viejos.
    try {
        if (-not ([System.Management.Automation.PSTypeName]"Win32.NotificacionEntorno").Type) {
            Add-Type -Namespace "Win32" -Name "NotificacionEntorno" -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
        }
        $resultado = [UIntPtr]::Zero
        $ret = [Win32.NotificacionEntorno]::SendMessageTimeout(
            [IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, "Environment", 0x0002, 5000, [ref]$resultado)
        return ($ret -ne [IntPtr]::Zero)
    } catch {
        return $false
    }
}

function Get-ValoresActuales {
    param([string]$Ambiente)   # "User" | "Machine"
    $mapa = [ordered]@{}
    try {
        $clave = if ($Ambiente -eq "User") { Get-Item -LiteralPath "HKCU:\Environment" }
                 else { Get-Item -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment" }
        foreach ($n in $clave.GetValueNames()) {
            $mapa[$n] = $clave.GetValue($n, "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        }
    } catch { }
    return $mapa
}

function Get-ValorBruto {
    # OJO: [Environment]::GetEnvironmentVariable EXPANDE los REG_EXPAND_SZ, asi que
    # %USERPROFILE%\AppData\Local\Temp salia como C:\Users\...\AppData\Local\Temp y la
    # verificacion daba falso negativo. Hay que leer el registro en crudo.
    param([string]$Nombre, [string]$Ambiente)
    try {
        $clave = if ($Ambiente -eq "User") { Get-Item -LiteralPath "HKCU:\Environment" }
                 else { Get-Item -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment" }
        return $clave.GetValue($Nombre, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    } catch { return $null }
}

function Compartir-Texto {
    param([string]$Texto, [int]$Max = 90)
    $t = ($Texto -replace '\s+', ' ').Trim()
    if ($t.Length -gt $Max) { return $t.Substring(0, $Max - 3) + "..." }
    return $t
}

# reg.exe escribe incluso en el exito ("The operation completed successfully") y con
# ErrorActionPreference = "Stop" eso se convierte en una excepcion terminante que
# aborta la restauracion a mitad. Por eso nunca se llama a reg.exe con 2>&1 directamente.
function Invoke-Reg {
    param([string[]]$Argumentos)
    $oldEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $salida = (& reg.exe @Argumentos 2>&1 | Out-String)
        return [pscustomobject]@{ ExitCode = $LASTEXITCODE; Text = $salida }
    } finally {
        $ErrorActionPreference = $oldEap
    }
}

function Invoke-NativoSilencioso {
    # Para comandos cuyo stderr es esperado (code --install-extension, etc.)
    param([string]$Comando, [string[]]$Argumentos)
    $oldEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $Comando @Argumentos 2>&1 | Out-Null; return $LASTEXITCODE }
    finally { $ErrorActionPreference = $oldEap }
}

# ============================================================================
#  1. LOCALIZAR EL BACKUP
# ============================================================================

$esAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
           ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

Write-Host ""
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  RESTAURAR ENTORNO DE DESARROLLO" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  Modo   : $(if ($SoloValidar) { 'SOLO VALIDAR (no modifica nada)' } else { 'RESTAURACION' })"
Write-Host "  Admin  : $(if ($esAdmin) { 'si' } else { 'NO (las variables de SISTEMA no se restauraran)' })"

if ([string]::IsNullOrWhiteSpace($CarpetaBackup)) {
    $raiz = Join-Path $PSScriptRoot "Backup"
    $candidatas = @(Get-ChildItem -Path $raiz -Directory -Filter "EnvBackup_*" -ErrorAction SilentlyContinue |
                    Sort-Object Name -Descending)
    if ($candidatas.Count -eq 0) {
        $CarpetaBackup = Read-Host "  Ruta de la carpeta del backup (ej. E:\Backups\EnvBackup_xxx)"
    } else {
        Write-Host ""
        Write-Host "  Backups disponibles:" -ForegroundColor Cyan
        for ($i = 0; $i -lt $candidatas.Count; $i++) {
            $marca = if ($i -eq 0) { "(mas reciente)" } else { "" }
            Write-Host ("    [{0}] {1}  {2}" -f $i, $candidatas[$i].Name, $marca)
        }
        $sel = Read-Host "  Numero a restaurar (o escribe la ruta completa)"
        if ($sel -match '^\d+$' -and [int]$sel -lt $candidatas.Count) {
            $CarpetaBackup = $candidatas[[int]$sel].FullName
        } else {
            $CarpetaBackup = $sel
        }
    }
}

$CarpetaBackup = $CarpetaBackup.Trim().Trim('"')
if (-not (Test-Path -LiteralPath $CarpetaBackup)) {
    Write-Host ""
    Write-Host "[FALLO] La carpeta '$CarpetaBackup' no existe." -ForegroundColor Red
    exit 1
}
$CarpetaBackup = (Resolve-Path -LiteralPath $CarpetaBackup).Path
Write-Host ""
Write-Host "  Backup: $CarpetaBackup" -ForegroundColor White

$regSistema   = Join-Path $CarpetaBackup "env-SISTEMA.reg"
$regUsuario   = Join-Path $CarpetaBackup "env-USUARIO.reg"
$dirInventario = Join-Path $CarpetaBackup "inventario"
$dirArchivos   = Join-Path $CarpetaBackup "archivos"

# ============================================================================
#  2. VERIFICAR QUE EL BACKUP ESTA COMPLETO
# ============================================================================

Write-Titulo "1. Verificacion del backup"

$problemas = @()

if (-not (Test-Path -LiteralPath $regUsuario)) {
    Write-Host "  [FALLO] No existe env-USUARIO.reg." -ForegroundColor Red
    exit 1
}
$tamUsuario = (Get-Item -LiteralPath $regUsuario).Length
if ($tamUsuario -lt 100) {
    $problemas += "env-USUARIO.reg parece vacio o corrupto ($tamUsuario bytes)"
    Write-Host "  [FALLO] env-USUARIO.reg esta vacio o corrupto ($tamUsuario bytes)." -ForegroundColor Red
    exit 1
}

$haySistema = Test-Path -LiteralPath $regSistema
if ($haySistema) {
    Write-Host "  [OK]     env-USUARIO.reg  ($tamUsuario bytes)" -ForegroundColor DarkGreen
    Write-Host "  [OK]     env-SISTEMA.reg  ($((Get-Item -LiteralPath $regSistema).Length) bytes)" -ForegroundColor DarkGreen
} else {
    Write-Host "  [AVISO]  No hay env-SISTEMA.reg en el backup." -ForegroundColor Yellow
    $problemas += "env-SISTEMA.reg ausente (las variables de SISTEMA no se restauraran)"
}
if (-not $esAdmin) {
    Write-Host "  [AVISO]  No eres administrador: env-SISTEMA.reg NO se importara." -ForegroundColor Yellow
    $problemas += "sin privilegios de administrador"
}

# Unidades de disco que el backup necesita y ya no existen
Write-Host ""
Write-Host "  Unidades de disco usadas por las variables del backup:" -ForegroundColor Cyan
$unidades = Get-UnidadesUsadas @($regUsuario, $regSistema)
$unidadesAusentes = @()
foreach ($u in $unidades) {
    $raiz = "${u}:\"
    if (Test-Path -LiteralPath $raiz) {
        Write-Host ("    [OK]    {0}" -f $raiz) -ForegroundColor DarkGreen
    } else {
        Write-Host ("    [FALLO] {0} NO EXISTE - las rutas que apuntan ahi quedaran rotas" -f $raiz) -ForegroundColor Red
        $unidadesAusentes += $u
        $problemas += "la unidad ${u}:\ no existe"
    }
}

# ============================================================================
#  3. LEER EL BACKUP Y MOSTRAR EL DIFF
# ============================================================================

Write-Titulo "2. Que va a cambiar"

$esperadoUsuario = Get-RegEntorno -RutaReg $regUsuario
$esperadoSistema = if ($haySistema) { Get-RegEntorno -RutaReg $regSistema } else { $null }
$actualUsuario   = Get-ValoresActuales -Ambiente "User"
$actualSistema   = Get-ValoresActuales -Ambiente "Machine"

function Show-Diff {
    param([System.Collections.IDictionary]$Esperado, [System.Collections.IDictionary]$Actual, [string]$Ambiente, [int]$MaxLineas = 12)

    $nuevas = @(); $distintas = @(); $iguales = 0
    foreach ($n in $Esperado.Keys) {
        $valorEsperado = $Esperado[$n].Valor
        $valorActual   = if ($Actual.Contains($n)) { $Actual[$n] } else { $null }
        if ($null -eq $valorActual) {
            $nuevas    += [pscustomobject]@{ Nombre = $n; De = "(no existe)"; A = $valorEsperado }
        } elseif ("$valorActual" -ne "$valorEsperado") {
            $distintas += [pscustomobject]@{ Nombre = $n; De = $valorActual; A = $valorEsperado }
        } else { $iguales++ }
    }
    $sobran = @($Actual.Keys | Where-Object { -not $Esperado.Contains($_) })

    Write-Host ""
    Write-Host "    $Ambiente : $($Esperado.Count) en el backup | $iguales iguales | $($nuevas.Count) nuevas | $($distintas.Count) con valor distinto | $($sobran.Count) solo en el equipo actual" -ForegroundColor Gray

    if ($iguales -gt 0) { Write-Host "    -> $iguales variable(s) ya estan correctas, no habra cambios en ellas." -ForegroundColor DarkGray }

    if ($nuevas.Count -gt 0) {
        Write-Host ""
        Write-Host "    NUEVAS (se anaden):" -ForegroundColor Yellow
        foreach ($x in ($nuevas | Select-Object -First $MaxLineas)) {
            Write-Host ("      + {0,-22} = {1}" -f $x.Nombre, (Compartir-Texto $x.A 70)) -ForegroundColor Yellow
        }
        if ($nuevas.Count -gt $MaxLineas) { Write-Host "      ... y $($nuevas.Count - $MaxLineas) mas" -ForegroundColor DarkGray }
    }
    if ($distintas.Count -gt 0) {
        Write-Host ""
        Write-Host "    VALOR DISTINTO (se sobrescribe):" -ForegroundColor Magenta
        foreach ($x in ($distintas | Select-Object -First $MaxLineas)) {
            Write-Host ("      ~ {0,-22}" -f $x.Nombre) -ForegroundColor Magenta
            Write-Host ("          actual  : {0}" -f (Compartir-Texto $x.De 70)) -ForegroundColor DarkGray
            Write-Host ("          backup  : {0}" -f (Compartir-Texto $x.A 70)) -ForegroundColor DarkGray
        }
        if ($distintas.Count -gt $MaxLineas) { Write-Host "      ... y $($distintas.Count - $MaxLineas) mas" -ForegroundColor DarkGray }
    }
    if ($sobran.Count -gt 0) {
        Write-Host ""
        Write-Host "    SOLO EN ESTE EQUIPO (NO se borran, la importacion solo anade/sobrescribe):" -ForegroundColor DarkGray
        Write-Host ("      $($sobran -join ', ')") -ForegroundColor DarkGray
    }
    return @($nuevas.Count + $distintas.Count)
}

$totalCambiosUsuario = Show-Diff -Esperado $esperadoUsuario -Actual $actualUsuario -Ambiente "USUARIO"
$totalCambiosSistema = 0
if ($esperadoSistema) {
    $totalCambiosSistema = Show-Diff -Esperado $esperadoSistema -Actual $actualSistema -Ambiente "SISTEMA"
} else {
    Write-Host ""
    Write-Host "    SISTEMA: sin datos de origen, no se puede comparar." -ForegroundColor DarkGray
}

if ($SoloValidar) {
    Write-Host ""
    Write-Host "  Modo -SoloValidar: no se ha modificado nada." -ForegroundColor Cyan
    if ($problemas.Count -gt 0) {
        Write-Host "  Problemas detectados:" -ForegroundColor Yellow
        $problemas | ForEach-Object { Write-Host "    - $_" -ForegroundColor Yellow }
    } else {
        Write-Host "  El backup esta completo y todas las unidades existen." -ForegroundColor DarkGreen
    }
    Write-Host ""
    exit 0
}

if (($totalCambiosUsuario + $totalCambiosSistema) -eq 0) {
    Write-Host ""
    Write-Host "  Las variables ya coinciden exactamente con el backup. No hay nada que hacer." -ForegroundColor DarkGreen
    Write-Host "  Si aun ves valores antiguos, reinicia sesion o el equipo (las apps abiertas no se refrescan)." -ForegroundColor Yellow
    Write-Host ""
    exit 0
}

# ============================================================================
#  4. PUNTO DE RESTAURACION DEL SISTEMA
# ============================================================================

if ($esAdmin -and -not $OmitirPuntoRestauracion) {
    Write-Titulo "3. Punto de restauracion del sistema"
    try {
        Enable-ComputerRestore -Drive "$env:SystemDrive\" -ErrorAction SilentlyContinue
        Checkpoint-Computer -Description "Antes de restaurar variables de entorno" -RestorePointType MODIFY_SETTINGS -ErrorAction Stop
        Write-Host "  [OK] Punto de restauracion creado." -ForegroundColor DarkGreen
    } catch {
        Write-Host "  [AVISO] No se pudo crear: $($_.Exception.Message)" -ForegroundColor Yellow
        Write-Host "          Windows limita a un punto cada 24h. Continuando igualmente." -ForegroundColor DarkGray
    }
} else {
    Write-Titulo "3. Punto de restauracion del sistema"
    Write-Host "  [OMITIDO] $($(if ($esAdmin) { 'usaste -OmitirPuntoRestauracion' } else { 'no eres administrador' }))" -ForegroundColor DarkGray
}

# ============================================================================
#  5. RESPALDO DE SEGURIDAD DEL ESTADO ACTUAL
# ============================================================================

Write-Titulo "4. Respaldo de seguridad del estado actual"
$safetyDir = Join-Path $CarpetaBackup ("pre-restore_" + (Get-Date -Format "yyyyMMdd_HHmmss"))
New-Item -ItemType Directory -Path $safetyDir -Force | Out-Null

$r = Invoke-Reg @("export", "HKCU\Environment", (Join-Path $safetyDir "actual-USUARIO.reg"), "/y")
if ($r.ExitCode -eq 0) { Write-Host "  [OK] actual-USUARIO.reg" -ForegroundColor DarkGreen }
else { Write-Host "  [AVISO] no se pudo respaldar HKCU\Environment" -ForegroundColor Yellow }

if ($esAdmin) {
    $r = Invoke-Reg @("export", "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment", (Join-Path $safetyDir "actual-SISTEMA.reg"), "/y")
    if ($r.ExitCode -eq 0) { Write-Host "  [OK] actual-SISTEMA.reg" -ForegroundColor DarkGreen }
    else { Write-Host "  [AVISO] no se pudo respaldar el registro del sistema" -ForegroundColor Yellow }
}

try {
    $clave = Get-Item -LiteralPath "HKCU:\Environment"
    $lineas = @("Estado actual de HKCU\Environment antes de restaurar ($([DateTime]::Now)):")
    foreach ($n in $clave.GetValueNames()) {
        $lineas += ("{0,-24} = {1}" -f $n, $clave.GetValue($n, "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames))
    }
    Set-Content -LiteralPath (Join-Path $safetyDir "actual-USUARIO-legible.txt") -Value ($lineas -join "`r`n") -Encoding utf8
    Write-Host "  [OK] actual-USUARIO-legible.txt" -ForegroundColor DarkGreen
} catch {
    Write-Host "  [AVISO] no se pudo generar el volcado legible: $($_.Exception.Message)" -ForegroundColor Yellow
}
Write-Host "  info   : $safetyDir" -ForegroundColor DarkGray

# ============================================================================
#  6. ADAPTAR EL NOMBRE DE USUARIO
# ============================================================================

Write-Titulo "5. Ajuste del nombre de usuario"

$usuarioActual = if ($UsuarioNuevo) { $UsuarioNuevo } else { $env:USERNAME }
$usuariosBackup = Get-UsuariosDetectados @($regUsuario, $regSistema)
$usuariosReales = @($usuariosBackup | Where-Object { $PerfilesDelSistema -notcontains $_ })

Write-Host "  Usuario actual de Windows : $env:USERNAME"
if ($UsuarioNuevo) { Write-Host "  Usuario destino forzado   : $usuarioActual" -ForegroundColor Yellow }
if ($usuariosReales.Count -gt 0) {
    Write-Host "  Carpetas de usuario del backup : $($usuariosReales -join ', ')"
} else {
    Write-Host "  El backup no contiene rutas C:\Users\... (no hay nada que ajustar)."
}

$aCambiar = @()
if (-not $NoAdaptarUsuario) {
    $aCambiar = @($usuariosReales | Where-Object { $_ -ne $usuarioActual } | Sort-Object -Property Length -Descending)
}
# Nombres que el script no puede deducir (p.ej. un perfil en la raiz de una unidad
# sin carpeta Users delante) pero que el usuario confirma como suyos.
foreach ($extra in $NombresExtra) {
    if ($extra -and ($extra -ne $usuarioActual) -and ($aCambiar -notcontains $extra)) {
        $aCambiar += $extra
    }
}
if ($NombresExtra.Count -gt 0) {
    $aCambiar = @($aCambiar | Sort-Object -Property Length -Descending)
    Write-Host ""
    Write-Host "  Nombres anadidos a mano con -NombresExtra: $($NombresExtra -join ', ')" -ForegroundColor Yellow
}

$regSistemaAplicar = $regSistema
$regUsuarioAplicar = $regUsuario

if ($aCambiar.Count -gt 0) {
    Write-Host ""
    Write-Host "  Se reescribira el nombre de usuario en TODAS las rutas, en cualquier unidad" -ForegroundColor Yellow
    Write-Host "  y en recursos de red, siempre que el nombre ocupe un segmento completo:" -ForegroundColor Yellow
    $aCambiar | ForEach-Object { Write-Host "      <segmento '$_'>  ->  <segmento '$usuarioActual'>" -ForegroundColor Yellow }
    Write-Host ""

    $aplicar = $Force
    if (-not $Force) {
        $r = Read-Host "  Aplicar este ajuste? (S/n)"
        $aplicar = ($r -ne "n" -and $r -ne "N")
    }
    if ($aplicar) {
        $dirAjustado = Join-Path $CarpetaBackup "ajustado_$usuarioActual"
        New-Item -ItemType Directory -Path $dirAjustado -Force | Out-Null
        $todosLosCambios = @()
        $detalleLineas = @()
        foreach ($par in @(@("SISTEMA", $regSistema), @("USUARIO", $regUsuario))) {
            $etiqueta = $par[0]; $origen = $par[1]
            if (-not (Test-Path -LiteralPath $origen)) { continue }
            $original = Get-Content -LiteralPath $origen -Raw
            $res = Ajustar-NombreEnReg -Contenido $original -NombresViejos $aCambiar -UsuarioNuevo $usuarioActual
            $todosLosCambios += $res.Resumen
            $detalleLineas += Get-InformeCambios -Antes $original -Despues $res.Contenido
            $destino = Join-Path $dirAjustado "env-$etiqueta-ajustado.reg"
            Set-Content -LiteralPath $destino -Value $res.Contenido -Encoding Unicode
            $nLineas = [int]($detalleLineas.Count / 2)
            Write-Host "  [OK] env-$etiqueta-ajustado.reg ($nLineas valores reescritos)" -ForegroundColor DarkGreen
        }

        # Informe detallado: diff real, para que puedas revisarlo antes de confirmar
        if ($todosLosCambios.Count -gt 0) {
            Write-Host ""
            Write-Host "  Nombres reescritos: $($todosLosCambios -join ', ')" -ForegroundColor Cyan
            Write-Host ""
            Write-Host "  Diff exacto (lo que cambia en cada valor):" -ForegroundColor Cyan
            $detalleLineas | Select-Object -First 30 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
            if ($detalleLineas.Count -gt 30) {
                Write-Host "    ... y $($detalleLineas.Count - 30) lineas mas" -ForegroundColor DarkGray
            }
        }

        # Carpetas tipo D:\Juan\... que NO tienen carpeta Users delante. No se pueden
        # distinguir de una carpeta de proyectos normal, asi que no se tocan solas:
        # se avisa y el usuario puede forzar el nombre con -UsuarioNuevo.
        $sospechosas = @{}
        foreach ($par in @(@("SISTEMA", $regSistema), @("USUARIO", $regUsuario))) {
            if (-not (Test-Path -LiteralPath $par[1])) { continue }
            $c = Get-Content -LiteralPath $par[1] -Raw
            foreach ($m in [regex]::Matches($c, '(?:[A-Z]:|\\\\\\\\[^\\"]+)\\\\+([^\\"]+?)(?=\\\\|")')) {
                $seg = $m.Groups[1].Value
                if (-not $seg) { continue }
                if ($CarpetasDelSistema -contains $seg) { continue }
                if ($aCambiar -contains $seg) { continue }
                if ($seg -eq $usuarioActual) { continue }
                if ($seg -match '\s') { continue }          # "Program Files" y similares
                if ($seg -notmatch '^[\w\.\-]{2,32}$') { continue }  # no parece un usuario
                if (-not $sospechosas.ContainsKey($seg)) { $sospechosas[$seg] = 0 }
                $sospechosas[$seg]++
            }
        }
        $sospechosas = @($sospechosas.GetEnumerator())
        if ($sospechosas.Count -gt 0) {
            Write-Host ""
            Write-Host "  [AVISO] Estas carpetas de la raiz de una unidad PODRIAN ser de tu usuario" -ForegroundColor Yellow
            Write-Host "          anterior, pero no es seguro deducirlo solo (una carpeta D:\Proyectos" -ForegroundColor Yellow
            Write-Host "          tambien encaja). No se han tocado. Revisa si son tuyas:" -ForegroundColor Yellow
            $sospechosas | Sort-Object -Property Value -Descending | Select-Object -First 8 | ForEach-Object {
                Write-Host ("            {0}   (aparecen en {1} sitio(s), p.ej. {0}:\...)" -f $_.Key, $_.Value) -ForegroundColor Yellow
            }
            Write-Host "          Si alguna de esas carpetas era la de tu usuario anterior, ejecuta" -ForegroundColor Yellow
            Write-Host "          otra vez anadiendola a mano:" -ForegroundColor Yellow
            $ejemplo = ($sospechosas | Sort-Object -Property Value -Descending | Select-Object -First 1).Key
            Write-Host "            -NombresExtra `"$ejemplo`"" -ForegroundColor Cyan
        }
        if (Test-Path -LiteralPath (Join-Path $dirAjustado "env-SISTEMA-ajustado.reg")) {
            $regSistemaAplicar = Join-Path $dirAjustado "env-SISTEMA-ajustado.reg"
        }
        if (Test-Path -LiteralPath (Join-Path $dirAjustado "env-USUARIO-ajustado.reg")) {
            $regUsuarioAplicar = Join-Path $dirAjustado "env-USUARIO-ajustado.reg"
        }
    } else {
        Write-Host "  [AVISO] Ajuste omitido. Las rutas C:\Users\ apuntaran a un usuario inexistente." -ForegroundColor Yellow
    }
} elseif ($usuariosReales.Count -gt 0) {
    Write-Host "  [OK] El nombre de usuario coincide, no hay que ajustar nada." -ForegroundColor DarkGreen
}

# La verificacion debe compararse contra el .reg que se va a IMPORTAR de verdad
# (con el nombre de usuario ya reescrito), no contra el original.
Write-Host ""
Write-Host "  La verificacion se hara sobre el archivo que se importa realmente." -ForegroundColor DarkGray
$esperadoUsuario = Get-RegEntorno -RutaReg $regUsuarioAplicar
$esperadoSistema = if ((Test-Path -LiteralPath $regSistemaAplicar)) { Get-RegEntorno -RutaReg $regSistemaAplicar } else { $null }

# ============================================================================
#  7. CONFIRMAR
# ============================================================================

Write-Titulo "6. Confirmacion"
Write-Host "  Se importara:"
if ($esAdmin -and (Test-Path -LiteralPath $regSistemaAplicar)) {
    Write-Host "    - $(Split-Path $regSistemaAplicar -Leaf)  -> variables de SISTEMA (HKLM)"
} elseif ($haySistema) {
    Write-Host "    - env-SISTEMA.reg  -> OMITIDA porque no eres administrador" -ForegroundColor Yellow
}
Write-Host "    - $(Split-Path $regUsuarioAplicar -Leaf)  -> variables de USUARIO (HKCU)"
Write-Host ""
Write-Host "  La importacion ANADE y SOBRESCRIBE valores. No borra variables que no esten en el backup."
Write-Host "  Estado actual guardado en: $safetyDir"
Write-Host ""

if (-not $Force) {
    $ok = Read-Host "  Confirmas la restauracion? (s/N)"
    if ($ok -ne "s" -and $ok -ne "S") {
        Write-Host "  Cancelado. No se ha modificado nada." -ForegroundColor Yellow
        Write-Host ""
        exit 0
    }
}

# ============================================================================
#  8. IMPORTAR
# ============================================================================

Write-Titulo "7. Aplicando cambios"

$fallosImport = @()

if ($esAdmin -and (Test-Path -LiteralPath $regSistemaAplicar)) {
    Write-Host "  Importando variables de SISTEMA..." -ForegroundColor Cyan
    $r = Invoke-Reg @("import", "$regSistemaAplicar")
    if ($r.ExitCode -eq 0) {
        Write-Host "  [OK] SISTEMA restaurado." -ForegroundColor DarkGreen
    } else {
        Write-Host "  [FALLO] Importacion de SISTEMA fallo (codigo $($r.ExitCode)):" -ForegroundColor Red
        Write-Host "        $(Compartir-Texto $r.Text 150)" -ForegroundColor Red
        $fallosImport += "SISTEMA"
    }
}

Write-Host "  Importando variables de USUARIO..." -ForegroundColor Cyan
$r = Invoke-Reg @("import", "$regUsuarioAplicar")
if ($r.ExitCode -eq 0) {
    Write-Host "  [OK] USUARIO restaurado." -ForegroundColor DarkGreen
} else {
    Write-Host "  [FALLO] Importacion de USUARIO fallo (codigo $($r.ExitCode)):" -ForegroundColor Red
    Write-Host "        $(Compartir-Texto $r.Text 150)" -ForegroundColor Red
    $fallosImport += "USUARIO"
}

# ============================================================================
#  9. NOTIFICAR A TODO EL SISTEMA
# ============================================================================

Write-Host ""
if (Send-NotificacionEntorno) {
    Write-Host "  [OK] Notificacion WM_SETTINGCHANGE enviada a las aplicaciones abiertas." -ForegroundColor DarkGreen
} else {
    Write-Host "  [AVISO] No se pudo enviar WM_SETTINGCHANGE. Reinicia sesion para estar seguro." -ForegroundColor Yellow
}

# ============================================================================
#  10. VERIFICAR VARIABLE POR VARIABLE
# ============================================================================

Write-Titulo "8. Verificacion"

function Test-Valores {
    param([System.Collections.IDictionary]$Esperado, [string]$Ambiente, [string]$Etiqueta)
    $bien = 0; $mal = @(); $perdidas = @()
foreach ($n in $Esperado.Keys) {
        $valorEsperado = $Esperado[$n].Valor
        $valorActual = Get-ValorBruto -Nombre $n -Ambiente $Ambiente
        if ($null -eq $valorActual) {
            if ("$valorEsperado" -eq "") { $bien++ } else { $perdidas += $n }
        } elseif ("$valorActual" -eq "$valorEsperado") {
            $bien++
        } else {
            $mal += [pscustomobject]@{ Nombre = $n; Esperado = $valorEsperado; Actual = $valorActual }
        }
    }
    Write-Host "  $Etiqueta : $bien/$($Esperado.Count) variables correctas" -ForegroundColor $(if ($mal.Count -eq 0 -and $perdidas.Count -eq 0) { "DarkGreen" } else { "Yellow" })
    if ($perdidas.Count -gt 0) {
        Write-Host "    [FALLO] no definidas: $($perdidas -join ', ')" -ForegroundColor Red
    }
    foreach ($x in $mal) {
        Write-Host "    [FALLO] $($x.Nombre)" -ForegroundColor Red
        Write-Host "             esperado: $(Compartir-Texto $x.Esperado 60)" -ForegroundColor DarkGray
        Write-Host "             actual  : $(Compartir-Texto $x.Actual 60)" -ForegroundColor DarkGray
    }
    return ($mal.Count + $perdidas.Count)
}

$errUsuario = Test-Valores -Esperado $esperadoUsuario -Ambiente "User" -Etiqueta "USUARIO"
$errSistema = 0
if ($esperadoSistema -and $esAdmin) {
    $errSistema = Test-Valores -Esperado $esperadoSistema -Ambiente "Machine" -Etiqueta "SISTEMA"
}

# ============================================================================
#  11. ARCHIVOS DE CONFIGURACION (opcional)
# ============================================================================

if ($RestaurarArchivos) {
    Write-Titulo "9. Restaurando archivos de configuracion"
    if (-not (Test-Path -LiteralPath $dirArchivos)) {
        Write-Host "  [AVISO] El backup no tiene carpeta 'archivos' (backup antiguo o hecho con -SoloInventario)." -ForegroundColor Yellow
    } else {
        $u = $env:USERPROFILE; $ap = $env:APPDATA
        $destinos = @(
            @{ Origen = "git\.gitconfig";   Destino = (Join-Path $u ".gitconfig");            Sobrescribir = $Force }
            @{ Origen = "git\.gitattributes"; Destino = (Join-Path $u ".gitattributes");     Sobrescribir = $Force }
            @{ Origen = "vscode\User\settings.json"; Destino = (Join-Path $ap "Code\User\settings.json");   Sobrescribir = $Force }
            @{ Origen = "vscode\User\keybindings.json"; Destino = (Join-Path $ap "Code\User\keybindings.json"); Sobrescribir = $Force }
            @{ Origen = "cursor\User\settings.json";  Destino = (Join-Path $ap "Cursor\User\settings.json"); Sobrescribir = $Force }
            @{ Origen = "android\debug.keystore"; Destino = (Join-Path $u ".android\debug.keystore");        Sobrescribir = $Force }
            @{ Origen = "cargo\config.toml"; Destino = (Join-Path $u ".cargo\config.toml");   Sobrescribir = $Force }
            @{ Origen = "maven\settings.xml"; Destino = (Join-Path $u ".m2\settings.xml");     Sobrescribir = $Force }
            @{ Origen = "gradle\gradle.properties"; Destino = ([Environment]::GetEnvironmentVariable("GRADLE_USER_HOME", "User")); Sobrescribir = $Force }
            @{ Origen = "node\.npmrc";   Destino = (Join-Path $u ".npmrc");                  Sobrescribir = $Force }
            @{ Origen = "dotnet\NuGet.Config"; Destino = (Join-Path $ap "NuGet\NuGet.Config"); Sobrescribir = $Force }
        )
        foreach ($d in $destinos) {
            $o = Join-Path $dirArchivos $d.Origen
            if (-not (Test-Path -LiteralPath $o -PathType Leaf)) { continue }
            $dest = $d.Destino
            if ([string]::IsNullOrWhiteSpace($dest)) { continue }
            if ((Test-Path -LiteralPath $dest) -and (-not $d.Sobrescribir)) {
                Write-Host ("  [OMITIDO] {0} (ya existe, no se sobrescribe sin -Force)" -f $d.Origen) -ForegroundColor DarkGray
                continue
            }
            try {
                $padre = Split-Path $dest -Parent
                if (-not (Test-Path -LiteralPath $padre)) { New-Item -ItemType Directory -Path $padre -Force | Out-Null }
                Copy-Item -LiteralPath $o -Destination $dest -Force
                Write-Host ("  [OK] {0} -> {1}" -f $d.Origen, $dest) -ForegroundColor DarkGreen
            } catch {
                Write-Host ("  [FALLO] {0}: {1}" -f $d.Origen, $_.Exception.Message) -ForegroundColor Red
            }
        }
        # Claves SSH: nunca se sobrescriben solas
        $sshOrigen = Join-Path $dirArchivos "ssh"
        $sshDestino = Join-Path $u ".ssh"
        if (Test-Path -LiteralPath $sshOrigen) {
            if ((Test-Path -LiteralPath $sshDestino) -and (-not $Force)) {
                Write-Host "  [OMITIDO] ssh/ ya existe en el equipo. Copialo a mano si lo necesitas." -ForegroundColor DarkGray
            } else {
                Copy-Item -LiteralPath $sshOrigen -Destination $sshDestino -Recurse -Force
                Write-Host "  [OK] claves SSH restauradas en $sshDestino" -ForegroundColor DarkGreen
            }
        }
        # Extensiones de editores
        foreach ($ed in @(@{ Area = "VS Code"; Cmd = "code"; Archivo = "vscode-extensiones.txt" },
                          @{ Area = "Cursor";  Cmd = "cursor"; Archivo = "cursor-extensiones.txt" })) {
            $lista = Join-Path $dirInventario $ed.Archivo
            if (-not (Test-Path -LiteralPath $lista)) { continue }
            if (-not (Get-Command $ed.Cmd -ErrorAction SilentlyContinue)) { continue }
            $exts = @(Get-Content -LiteralPath $lista | Where-Object { $_ -match '@' } | ForEach-Object { ($_ -split '@')[0] })
            if ($exts.Count -eq 0) { continue }
Write-Host "  Instalando $($exts.Count) extensiones de $($ed.Area)..." -ForegroundColor Cyan
            $falladas = 0
            foreach ($e in $exts) {
                $code = Invoke-NativoSilencioso -Comando $ed.Cmd -Argumentos @('--install-extension', $e, '--force')
                if ($code -ne 0) { $falladas++; Write-Host "    [fallo] $e (exit $code)" -ForegroundColor Yellow }
            }
            if ($falladas -eq 0) {
                Write-Host "  [OK] $($ed.Area): $($exts.Count) extensiones instaladas" -ForegroundColor DarkGreen
            } else {
                Write-Host "  [AVISO] $($ed.Area): $($exts.Count - $falladas) instaladas, $falladas fallaron" -ForegroundColor Yellow
            }
        }
    }
}

# ============================================================================
#  12. SIGUIENTES PASOS
# =====================================================================================

Write-Titulo "10. Siguientes pasos"

$guia = Join-Path $CarpetaBackup "COMO-RESTAURAR.txt"
if (Test-Path -LiteralPath $guia) {
    Write-Host "  Tienes la guia completa de reinstalacion de SDKs en:" -ForegroundColor Cyan
    Write-Host "    $guia" -ForegroundColor White
}

$pasos = @()
if (Test-Path -LiteralPath (Join-Path $dirInventario "apps-winget.json")) {
    $pasos += "  winget import --import-file `"$dirInventario\apps-winget.json`""
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "vscode-extensiones.txt")) {
    $pasos += "  code --install-extension <id>        (ver inventario\vscode-extensiones.txt)"
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "rustup-toolchains.txt")) {
    $pasos += "  rustup toolchain install <los de inventario\rustup-toolchains.txt>"
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "android-paquetes-instalados.txt")) {
    $pasos += "  sdkmanager <paquetes de inventario\android-paquetes-instalados.txt>"
    $pasos += "  Las licencias ya estan copiadas en .\archivos\android\licenses"
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "android-ndk-toolchain.txt")) {
    $pasos += "  Android/Tauri: comprueba que el PATH incluye la carpeta"
    $pasos += "  <NDK>\toolchains\llvm\prebuilt\<host>\bin del inventario\android-ndk-toolchain.txt"
    $pasos += "  (sin ella, 'tauri android build' falla con cargo build codigo 101)"
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "flutter-version.txt")) {
    $pasos += "  Reinstalar Flutter y luego: flutter doctor"
}
if (Test-Path -LiteralPath (Join-Path $dirInventario "jdk-instalados.txt")) {
    $pasos += "  Reinstalar el JDK y ajustar JAVA_HOME (ver inventario\jdk-instalados.txt)"
}
if ($pasos.Count -gt 0) {
    Write-Host ""
    Write-Host "  Comandos sugeridos:" -ForegroundColor Cyan
    $pasos | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
}

Write-Host ""
Write-Host "  1) CIERRA TODAS LAS TERMINALES, VS CODE, EL EXPLORER y vuelve a abrir." -ForegroundColor Yellow
Write-Host "  2) Si aun ves valores viejos: cierra sesion y vuelve a entrar." -ForegroundColor Yellow
Write-Host "  3) Comprueba con:  `$env:JAVA_HOME   /   `$env:ANDROID_HOME" -ForegroundColor Yellow

# ============================================================================
#  RESUMEN  (siempre se imprime)
# ============================================================================

Write-Host ""
Write-Host "============================================================" -ForegroundColor DarkCyan
Write-Host "  RESUMEN" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor DarkCyan

$estado = if ($fallosImport.Count -eq 0 -and $errUsuario -eq 0 -and $errSistema -eq 0) { "COMPLETADO" } else { "CON ERRORES" }
$colorEstado = if ($estado -eq "COMPLETADO") { "DarkGreen" } else { "Red" }
Write-Host "  Estado      : $estado" -ForegroundColor $colorEstado
Write-Host "  Import      : $(if ($fallosImport.Count) { "fallos en $($fallosImport -join ', ')" } else { 'sin errores' })"
Write-Host "  Verificacion: $(if (($errUsuario + $errSistema) -eq 0) { 'todas las variables coinciden con el backup' } else { "$($errUsuario + $errSistema) variable(s) no coinciden" })"
if (-not $esAdmin) { Write-Host "  Sistema     : NO restaurado (hacia falta ser administrador)" -ForegroundColor Yellow }
if ($unidadesAusentes.Count -gt 0) { Write-Host "  Unidades ausentes: $($unidadesAusentes -join ', ') - revisa las rutas afectadas" -ForegroundColor Yellow }
Write-Host "  Respaldo del estado previo: $safetyDir"
Write-Host "  Tiempo total: $([int]((Get-Date) - $script:Inicio).TotalSeconds) s"
Write-Host ""

if ($errUsuario -gt 0 -or $errSistema -gt 0 -or $fallosImport.Count -gt 0) {
    Write-Host "  Para volver atras, importa a mano los .reg de esta carpeta:" -ForegroundColor Yellow
    Write-Host "    reg import `"$safetyDir\actual-USUARIO.reg`"" -ForegroundColor DarkGray
    if ($esAdmin) { Write-Host "    reg import `"$safetyDir\actual-SISTEMA.reg`"" -ForegroundColor DarkGray }
    Write-Host ""
    exit 1
}
exit 0
