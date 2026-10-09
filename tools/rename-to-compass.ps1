# Renombre de "code-runner" a "compass" en el texto de todos los ficheros.
# Los scripts shell sin extensión se cubren filtrando todos los ficheros bajo
# assets/scripts/ y todos los .in/.sh/.bat/.rs/.toml/.md/.lock/.py/.ps1.
$ErrorActionPreference = 'Stop'
$raiz = "D:\Mis Proyectos\Rust\Developing Runner"

$pares = @(
    ,@('zz_CodeRunner',  'zz_Compass')
    ,@('CODE_RUNNER_NAME','COMPASS_NAME')
    ,@('CR_WRAPPER',     'COMPASS_WRAPPER')
    ,@('code_runner_exe','compass_exe')
    ,@('code-runner',    'compass')
    ,@('code_runner',    'compass')
    ,@('Code Runner',    'Compass')
    ,@('Code-Runner',    'Compass')
    ,@('CodeRunner',     'Compass')
)

$ficheros = (Get-ChildItem -Path $raiz -Recurse -File) |
    Where-Object {
        $_.FullName -notmatch '\\target\\' -and
        $_.FullName -notmatch '\\\.git\\' -and
        $_.FullName -notmatch '\\tools\\rename-to-compass\.ps1$' -and
        $_.FullName -notmatch '\\icon\\' -and
        ($_.Extension -in @('.rs', '.toml', '.lock', '.md', '.bat', '.sh', '.py', '.ps1', '.in') -or
         $_.FullName -match '\\assets\\scripts\\(crh|code-runner-open|run-sh)$')
    }

foreach ($f in $ficheros) {
    $texto = [System.IO.File]::ReadAllText($f.FullName, [System.Text.Encoding]::UTF8)
    $original = $texto
    foreach ($par in $pares) {
        $texto = $texto.Replace($par[0], $par[1])
    }
    if ($texto -ne $original) {
        [System.IO.File]::WriteAllText($f.FullName, $texto, (New-Object System.Text.UTF8Encoding($false)))
        Write-Host ("OK: {0}" -f ($f.FullName.Substring($raiz.Length)))
    }
}

Write-Host "=== sustitución completada ==="