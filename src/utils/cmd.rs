use std::process::{Command, Output};
use anyhow::{Context, Result};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn run(cmd: &str, args: &[&str]) -> Result<Output> {
    Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("Falló al ejecutar: {} {}", cmd, args.join(" ")))
}

pub fn run_in_dir(cmd: &str, args: &[&str], dir: &std::path::Path) -> Result<Output> {
    if !dir.exists() {
        anyhow::bail!("El directorio no existe: {}", dir.display());
    }
    Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("Falló al ejecutar: {} {} en {:?}", cmd, args.join(" "), dir))
}

pub fn run_status(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("Falló al ejecutar: {} {}", cmd, args.join(" ")))?;
    if !status.success() {
        anyhow::bail!("Comando falló con código: {}", status);
    }
    Ok(())
}

pub fn run_in_dir_status(cmd: &str, args: &[&str], dir: &std::path::Path) -> Result<()> {
    if !dir.exists() {
        anyhow::bail!("El directorio no existe: {}", dir.display());
    }
    let status = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("Falló al ejecutar '{}'. Verifica que el programa esté instalado y en el PATH.", cmd))?;
    if !status.success() {
        anyhow::bail!("Comando falló con código: {}", status);
    }
    Ok(())
}

/// Ejecuta `cmd` esperando como máximo `timeout_secs` y con el **stdin cerrado**.
///
/// Existe para los comandos que pueden quedarse esperando una confirmación
/// interactiva: `Install-Module` pregunta por consola si no confía en el
/// repositorio, y PSGallery pide aceptar la licencia la primera vez. Con el stdin
/// a `Stdio::null()` esa pregunta lee EOF y aborta, en vez de dejar la TUI colgada
/// para siempre; el techo de tiempo cubre el caso de que el hijo se quede
/// bloqueado por otra razón (la primera descarga del proveedor NuGet).
///
/// Devuelve `Ok(false)` si se agotó el tiempo (el proceso se mata), y `Err` si no
/// se pudo ni arrancar.
pub fn run_timeout(cmd: &str, args: &[&str], timeout_secs: u64) -> Result<bool> {
    use std::time::{Duration, Instant};

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("Falló al ejecutar '{}'. Verifica que el programa esté instalado y en el PATH.", cmd))?;

    let limite = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status.success()),
            None if Instant::now() >= limite => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(false);
            }
            None => std::thread::sleep(Duration::from_millis(150)),
        }
    }
}

/// Ejecuta npm install en un directorio.
/// En Windows, npm es npm.cmd y requiere cmd.exe para ejecutarse.
/// Esta función lo resuelve automáticamente.
pub fn run_npm_install(dir: &std::path::Path) -> Result<()> {
    if !dir.exists() {
        anyhow::bail!("El directorio no existe: {}", dir.display());
    }
    if !dir.join("package.json").exists() {
        anyhow::bail!("No se encontró package.json en: {}", dir.display());
    }

    let program = if cfg!(windows) { "cmd" } else { "npm" };
    let args: &[&str] = if cfg!(windows) {
        &["/c", "npm.cmd", "install"]
    } else {
        &["install"]
    };

    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| {
            format!(
                "No se pudo ejecutar npm. Verifica que Node.js esté instalado.\n\
                 Ruta esperada del proyecto: {}",
                dir.display()
            )
        })?;

    if !status.success() {
        anyhow::bail!("npm install falló en: {}", dir.display());
    }
    Ok(())
}

// ── Spawn de ventanas ─────────────────────────────────────────────────
//
// En Windows: `powershell` / `cmd` con `CREATE_NEW_CONSOLE`.
// En Linux: se delega en un emulador de terminal detectado (`gnome-terminal`,
// `konsole`, `xterm`, ...). Sin esto, `spawn_new_window*` heredaba la terminal
// de la TUI y la pintaba encima, dejándola inutilizable.

/// Emulador de terminal conocido, con su forma exacta de invocar un comando.
///
/// **El orden es la preferencia**: se usa el primero que esté en el PATH.
///
/// `alacritty` va primero por defecto porque, a diferencia de `cosmic-term`,
/// implementa el modo de ratón **1003 (all-motion)**. Eso importa para el menú
/// TUI: con 1002 (sólo movimiento con botón pulsado) el hover de las cards no
/// se activa nunca, porque el terminal no envía ningún evento.
///
/// Ver `MouseReporter` en `cosmic-term/src/mouse_reporter.rs`: para
/// `CursorMoved` hace `self.button.map(...)`, y `Option::map` sobre `None`
/// devuelve `None` — sin botón no se envía nada.
///
/// Los tres campos separan lo que antes se mezclaba en un único `&[&str]`:
/// - `lead`: argumentos previos, sólo para los que lanzan subcomando
///   (`wezterm start --`).
/// - `title_flag`: flag del título, o `None` si el terminal no lo expone.
/// - `exec_flag`: flag que introduce el comando a ejecutar.
///
/// `wezterm` necesita `lead` porque no acepta `--title`; con el modelo anterior
/// (`&["start", "--"]` playing el papel de flag de título) se le pasaba
/// `'título' -e ...` como argumentos sueltos.
#[cfg(not(windows))]
struct TerminalSpec {
    bin: &'static str,
    lead: &'static [&'static str],
    title_flag: Option<&'static str>,
    exec_flag: &'static str,
}

#[cfg(not(windows))]
const TERMINALS: &[TerminalSpec] = &[
    // kitty va primero porque es el único ligero que tiene a la vez las dos
    // cosas que necesita el TUI: modo de ratón 1003 (hover de las cards) y
    // pestañas de verdad. Ni alacritty ni cosmic-term tienen pestañas — no es
    // una opción de configuración, la funcionalidad no existe en Linux.
    TerminalSpec { bin: "kitty",           lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "alacritty",       lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "cosmic-term",     lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "gnome-terminal",  lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "konsole",         lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "xfce4-terminal",  lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "kitty",           lead: &[],                title_flag: Some("--title"), exec_flag: "-e" },
    TerminalSpec { bin: "wezterm",         lead: &["start", "--"],    title_flag: None,           exec_flag: "" },
    TerminalSpec { bin: "xterm",           lead: &[],                title_flag: Some("-T"),      exec_flag: "-e" },
];

/// Primer emulador de terminal disponible en el PATH.
///
/// Se busca en el orden de [`TERMINALS`], que pone primero los de Pop!_OS.
#[cfg(not(windows))]
fn detect_terminal() -> Option<&'static TerminalSpec> {
    TERMINALS.iter().find(|spec| {
        Command::new("sh")
            .args(["-c", &format!("command -v {}", spec.bin)])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Base del socket IPC de kitty, tal y como se declara en su `kitty.conf`
/// (`listen_on unix:@kitty-compass`).
#[cfg(not(windows))]
const KITTY_SOCKET_PREFIX: &str = "@kitty-compass";

/// Dirección del socket IPC de una instancia de kitty viva, si la hay.
///
/// kitty crea el socket a partir de `listen_on unix:@kitty-compass` y le
/// **añade el PID**: `@kitty-compass-<pid>`. Por eso no puede usarse una
/// ruta fija. El nombre va sin barra inicial, o sea que es relativo, y en la
/// práctica **no aparece como fichero en disco** (se ve en `ss` como
/// `LISTEN @kitty-compass-<pid>`, pero `find -type s` no lo encuentra), así
/// que la única forma fiable de descubrirlo es leer la tabla de sockets.
///
/// Se elige el PID más alto, que es la instancia creada más recientemente: es la
/// que el usuario tiene delante.
///
/// Devuelve `None` si no hay kitty abierto, que es el caso normal cuando
/// compass se lanza desde el launcher sin ninguna ventana de kitty previa.
#[cfg(not(windows))]
fn kitty_socket() -> Option<String> {
    let out = Command::new("ss")
        .args(["-xlH"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);

    let mut best: Option<(u32, String)> = None;
    for token in text.split_whitespace() {
        let Some(pid) = token
            .strip_prefix(KITTY_SOCKET_PREFIX)
            .and_then(|s| s.strip_prefix('-'))
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if best.as_ref().map_or(true, |(bp, _)| pid > *bp) {
            best = Some((pid, token.to_string()));
        }
    }
    best.map(|(_, name)| format!("unix:{name}"))
}

/// Abre `script` como **pestaña** de la ventana de kitty que ya está abierta.
///
/// Devuelve `false` si no hay instancia kitty con la que hablar, y entonces el
/// llamante debe abrir una ventana nueva.
///
/// Nota sobre por qué hace falta el socket: `kitty @ launch` busca el socket en
/// `$KITTY_LISTEN_ON` y, si no lo encuentra, envía el mensaje a la terminal
/// controladora — o sea, sólo funciona dentro de una ventana de kitty. Como
/// compass se lanza desde un terminal cualquiera (alacritty, cosmic-term), no
/// encontraría nada, de ahí el `--to` explícito.
#[cfg(not(windows))]
fn spawn_kitty_tab(title: &str, script: &str) -> bool {
    let Some(socket) = kitty_socket() else { return false };

    // `kitty @` necesita un terminal para trabajar; `script` le presta uno
    // descartable. Sin él responde "open /dev/tty: no such device or address".
    // `script -c` ejecuta el comando y propaga su código de salida.
    let status = Command::new("script")
        .arg("-qec")
        .arg(format!(
            "kitty @ --to {socket} launch --type=tab --title {} sh -lc {}",
            sh_quote(title),
            sh_quote(script)
        ))
        .arg("/dev/null")
        .stdin(std::process::Stdio::null())
        .status();

    matches!(status, Ok(s) if s.success())
}

/// Cuerpo de shell que lanza `script` decidiendo en **tiempo de ejecución**
/// entre pestaña de kitty y ventana nueva.
///
/// Existe porque los wrappers `vibe-*` se generan una sola vez y se quedan
/// escritos en disco, pero en el momento de generarlos no se sabe si el
/// usuario tendrá kitty abierto: si se hornea el argv con
/// [`terminal_prefix`], siempre abrirá una ventana nueva. En vez de eso se emite
/// el condicional y el wrapper decide cuando se ejecuta.
///
/// ⚠️ **La rama de la pestaña parsea el script dos veces, y está roto.** El
/// `script -qec "… sh -lc \"$CR_SCRIPT\""` de abajo llega al shell intermedio
/// como una cadena con las comillas ya consumidas, así que ese shell vuelve a
/// parsearla: `$target` sale vacío (no está definido ahí) y la división por IFS
/// trunca el comando. La rama de ventana nueva sí funciona, porque kitty hace
/// `exec` directo de `sh -lc <script>` sin re-parsear.
///
/// Arreglo propuesto: exportar `CR_SCRIPT`/`CR_SOCKET`/`CR_TITLE` y pasarle a
/// `script -qec` **un argumento con comillas simples**, para que cite el shell
/// intermedio. Está sin aplicar porque no se ha podido probar con un kitty real:
/// hace falta `ss -xlH | grep kitty-compass` listando
/// `@kitty-compass-<pid>`, o sea una instancia de kitty ya abierta. Ver
/// "Bug conocido: doble parseo en la rama kitty-tab" en AGENTS.md.
///
/// Devuelve `None` si no hay ningún emulador instalado.
#[cfg(not(windows))]
pub fn terminal_launch_shell(title: &str, script: &str) -> Option<String> {
    let spec = detect_terminal()?;

    if spec.bin == "kitty" {
        let quoted_title = sh_quote(title);
        // El script NO va embebido en la cadena de `script -qec`: esa cadena
        // usa comillas dobles, así que un `"$target"` o un `read -r` de dentro
        // del script rompen el quoting y `script` aborta con "unexpected number
        // of arguments". Se pasa por variable de entorno y se referencia como
        // `$CR_SCRIPT`, que no se expande donde no toca.
        return Some(format!(
            r#"socket="$(ss -xlH 2>/dev/null | tr ' ' '\n' | grep -m1 '^{prefix}-[0-9]*$' || true)"
export CR_SCRIPT=$(cat <<'CR_EOF'
{script}
CR_EOF
)
if [ -n "$socket" ]; then
    exec script -qec "kitty @ --to unix:$socket launch --type=tab --title {qt} sh -lc \"$CR_SCRIPT\"" /dev/null
fi
# Sin instancia de kitty con la que hablar, se abre una ventana nueva.
exec kitty --title {qt} -e sh -lc "$CR_SCRIPT""#,
            prefix = KITTY_SOCKET_PREFIX,
            qt = quoted_title,
            script = script,
        ));
    }

    let argv = terminal_prefix(title, script)?;
    let cmd_line = argv
        .iter()
        .map(|a| sh_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!("exec {cmd_line}"))
}

/// En Windows no hay emulador externo que lanzar: ver [`spawn_in_terminal`].
#[cfg(windows)]
pub fn terminal_launch_shell(_title: &str, _script: &str) -> Option<String> {
    None
}

/// Prefijo de argv para abrir `script` en una ventana de terminal nueva.
///
/// Devuelve `None` si no hay ningún emulador instalado. Es la mitad "cómo
/// invocar un terminal" de [`spawn_in_terminal`], expuesta por separado porque
/// `context_menu_cosmic` la necesita para incrustarla en los wrappers `vibe-*`
/// (que corren sin tty y deben abrir su propia ventana).
///
/// En Windows devuelve `None`: allí las ventanas se abren con
/// `CREATE_NEW_CONSOLE`, no con un emulador.
#[cfg(not(windows))]
pub fn terminal_prefix(title: &str, script: &str) -> Option<Vec<String>> {
    let spec = detect_terminal()?;
    let mut argv: Vec<String> = Vec::with_capacity(8);
    argv.push(spec.bin.to_string());
    argv.extend(spec.lead.iter().map(|s| s.to_string()));
    if let Some(flag) = spec.title_flag {
        argv.push(flag.to_string());
        argv.push(title.to_string());
    }
    if !spec.exec_flag.is_empty() {
        argv.push(spec.exec_flag.to_string());
    }
    argv.push("sh".to_string());
    argv.push("-lc".to_string());
    argv.push(script.to_string());
    Some(argv)
}

/// Escapa una cadena para incrustarla entre comillas simples en POSIX `sh`.
#[cfg(not(windows))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Envuelve `script` en un emulador de terminal nuevo y lo lanza.
#[cfg(not(windows))]
fn spawn_in_terminal(title: &str, script: &str) -> Result<()> {
    // Si hay una ventana de kitty abierta y el terminal elegido es kitty, se
    // prefiere abrir una PESTAÑA en ella en vez de otra ventana. Si no hay
    // instancia kitty (típico al arrancar desde el launcher), se cae a la
    // ventana nueva de siempre.
    if detect_terminal().map(|s| s.bin) == Some("kitty") && spawn_kitty_tab(title, script) {
        return Ok(());
    }

    let argv = terminal_prefix(title, script).with_context(|| {
        "[ERROR] No se encontró ningún emulador de terminal.\n\
         Instala uno de: cosmic-term, gnome-terminal, konsole, xfce4-terminal, alacritty, kitty, wezterm, xterm"
    })?;

    let bin = argv[0].clone();
    let mut cmd = Command::new(&bin);
    cmd.args(&argv[1..]);

    // La ventana nueva debe sobrevivir a la que la lanzó. Sin esto el emulador
    // nace en el mismo grupo de procesos y con los mismos stdio que el pty del
    // padre, así que cuando esa ventana se cierra (variante `autoexit`, que
    // muere nada más después de lanzar esta) el hijo se va con ella: la ventana
    // se abre y se cierra al instante.
    //
    // `process_group(0)` lo saca del grupo, y los stdio a /dev/null lo desligan
    // del pty del padre. Es el equivalente a `setsid … </dev/null >/dev/null`.
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    cmd.spawn()
        .with_context(|| format!("No se pudo abrir una ventana de {bin}"))?;
    Ok(())
}

/// En Windows no hay emulador externo que lanzar: ver `spawn_in_terminal`.
///
/// Se mantiene por simetría con la variante POSIX, aunque hoy nada la llama en
/// Windows: los wrappers de COSMIC van por [`terminal_launch_shell`], que aquí
/// devuelve `None` y hace que el script caiga al aviso de "sin tty".
#[cfg(windows)]
#[allow(dead_code)]
pub fn terminal_prefix(_title: &str, _script: &str) -> Option<Vec<String>> {
    None
}

/// Abre una ventana de terminal que **permanece abierta** al terminar el comando.
pub fn spawn_new_window(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let ps_command = format!(
            "$host.UI.RawUI.WindowTitle = '{}'; Set-Location '{}'; {}",
            title,
            dir.display(),
            command
        );
        Command::new("powershell")
            .creation_flags(CREATE_NEW_CONSOLE)
            .args(["-NoExit", "-Command", &ps_command])
            .spawn()
            .context("No se pudo abrir nueva ventana de PowerShell")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let script = format!(
            "cd {} && {} ; echo; echo 'Presiona Enter para cerrar...'; read -r",
            sh_quote(&dir.display().to_string()),
            command
        );
        spawn_in_terminal(title, &script)
    }
}

/// Abre una ventana de terminal que **se cierra** al terminar el comando.
pub fn spawn_new_window_autoexit(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let ps_command = format!(
            "$host.UI.RawUI.WindowTitle = '{}'; Set-Location '{}'; {}",
            title,
            dir.display(),
            command
        );
        Command::new("powershell")
            .creation_flags(CREATE_NEW_CONSOLE)
            .args(["-Command", &ps_command])
            .spawn()
            .context("No se pudo abrir nueva ventana de PowerShell")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // El emulador de terminal cierra la ventana solo cuando el hijo termina.
        let script = format!(
            "cd {} && {}",
            sh_quote(&dir.display().to_string()),
            command
        );
        spawn_in_terminal(title, &script)
    }
}

/// Abre una nueva terminal con stdin funcional para comandos interactivos
/// (flutter run, gradle run, etc.). Internamente usa cmd /c en vez de
/// powershell -Command porque este último no reenvía correctamente el
/// teclado a procesos hijo en ciertas configuraciones de Windows.
pub fn spawn_new_window_interactive(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let cmd_line = format!("title {} & {} & pause", title, command);
        Command::new("cmd")
            .creation_flags(CREATE_NEW_CONSOLE)
            .current_dir(dir)
            .args(["/c", &cmd_line])
            .spawn()
            .context("No se pudo abrir nueva ventana de CMD")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let script = format!(
            "cd {} && {} ; echo; echo 'Presiona Enter para cerrar...'; read -r",
            sh_quote(&dir.display().to_string()),
            command
        );
        spawn_in_terminal(title, &script)
    }
}

/// Expresión `find` (GNU) que localiza el archivo más reciente en `folder`
/// cuya extensión esté en `exts`, y lo deja en la variable `f`.
///
/// Usa `-printf '%T@ %p\n'` + `sort -rn` en vez de `ls -t` porque el orden por
/// fecha de `ls` no es estable entre versiones de coreutils.
#[cfg(not(windows))]
fn sh_newest_artifact(folder: &std::path::Path, exts: &[&str]) -> String {
    let dir = sh_quote(&folder.display().to_string());
    let names = if exts.is_empty() {
        // Sin extensiones que filtrar: empareja cualquier archivo regular.
        "-type f".to_string()
    } else {
        let patterns = exts
            .iter()
            .map(|e| format!("-name '*.{e}'"))
            .collect::<Vec<_>>()
            .join(" -o ");
        format!("-type f \\( {patterns} \\)")
    };
    format!(
        "f=$(find {dir} {names} -printf '%T@ %p\\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)"
    )
}

/// Genera un snippet `sh` que renombra el artefacto más reciente en `folder`
/// a `new_name` (conservando su extensión). Deja el nuevo path en `f`.
#[cfg(not(windows))]
fn rename_artifact_snippet(folder: &std::path::Path, exts: &[&str], new_name: &str) -> String {
    let base = sh_quote(new_name);
    format!(
        "{find}; if [ -n \"$f\" ]; then ext=\"${{f##*.}}\"; target=\"$(dirname \"$f\")/{base}.$ext\"; mv \"$f\" \"$target\" && f=\"$target\"; fi",
        find = sh_newest_artifact(folder, exts),
        base = base,
    )
}

/// Genera un snippet PowerShell que renombra el artefacto más reciente
/// en `folder` (con las extensiones dadas) a `new_name` (sin extensión).
/// Actualiza la variable `$f` para que apunte al archivo renombrado.
#[cfg(windows)]
fn rename_artifact_snippet(folder: &std::path::Path, exts: &[&str], new_name: &str) -> String {
    let folder_ps = folder.display().to_string().replace('\'', "''");
    let ext_list = exts
        .iter()
        .map(|e| format!("'.{}'", e))
        .collect::<Vec<_>>()
        .join(",");
    let safe_name = new_name.replace('\'', "''");
    format!(
        "$f = Get-ChildItem -LiteralPath '{}' -Recurse -File | Where-Object {{ $_.Extension -in {} }} | Sort-Object LastWriteTime -Descending | Select-Object -First 1; if ($f) {{ $newName = '{}' + $f.Extension; Rename-Item -LiteralPath $f.FullName -NewName $newName; $f = Get-Item -LiteralPath (Join-Path $f.DirectoryName $newName) }}",
        folder_ps, ext_list, safe_name
    )
}

/// Envuelve un comando de build para que, al terminar con éxito, abra el
/// explorador de archivos apuntando al artefacto más reciente (exe, apk, aab)
/// dentro de `folder`. Si el build falla, muestra un error.
/// La ventana NUNCA se cierra sola para que el usuario pueda ver toda la salida.
/// Si `rename_to` se provee, renombra el artefacto antes de abrir el explorador.
///
/// Windows genera PowerShell; Linux genera `sh` (para `spawn_new_window*`,
/// que en Linux ejecutan el script con `sh -lc`).
pub fn reveal_on_success(
    command: &str,
    folder: &std::path::Path,
    exts: &[&str],
    rename_to: Option<&str>,
) -> String {
    #[cfg(windows)]
    {
        let folder_ps = folder.display().to_string().replace('\'', "''");
        let ext_list = exts
            .iter()
            .map(|e| format!("'.{}'", e))
            .collect::<Vec<_>>()
            .join(",");

        let rename_snippet = match rename_to {
            Some(name) => format!("{}; ", rename_artifact_snippet(folder, exts, name)),
            None => String::new(),
        };

        format!(
            "{}; if ($?) {{ {}$f = Get-ChildItem -LiteralPath '{}' -Recurse -File | Where-Object {{ $_.Extension -in {} }} | Sort-Object LastWriteTime -Descending | Select-Object -First 1; if ($f) {{ Write-Host ''; Write-Host ('[OK] Build exitoso: ' + $f.Name) -ForegroundColor Green; Write-Host ('[INFO] Artefacto: ' + $f.FullName) -ForegroundColor Cyan; explorer.exe (\"/select,`\"\" + $f.FullName + \"`\"\") }} else {{ Write-Host ''; Write-Host '[OK] Build exitoso (artefacto no encontrado en la carpeta)' -ForegroundColor Green; explorer.exe '{}' }} }} else {{ Write-Host ''; Write-Host '[ERROR] El build fallo.' -ForegroundColor Red }}; Write-Host ''; Write-Host 'Presiona Enter para cerrar...' -ForegroundColor Yellow; Read-Host",
            command, rename_snippet, folder_ps, ext_list, folder_ps
        )
    }
    #[cfg(not(windows))]
    {
        let dir = sh_quote(&folder.display().to_string());

        let rename_snippet = match rename_to {
            Some(name) => format!("{}; ", rename_artifact_snippet(folder, exts, name)),
            None => String::new(),
        };

        format!(
            "{command}; \
             if [ $? -eq 0 ]; then \
               {rename_snippet}{find}; \
               if [ -n \"$f\" ]; then \
                 echo; printf '\\033[32m[OK] Build exitoso: %s\\033[0m\\n' \"$(basename \"$f\")\"; \
                 printf '\\033[36m[INFO] Artefacto: %s\\033[0m\\n' \"$f\"; \
                 xdg-open \"$f\" 2>/dev/null || gio open \"$f\" 2>/dev/null || echo \"[WARN] No se pudo abrir el explorador de archivos\"; \
               else \
                 echo; printf '\\033[32m[OK] Build exitoso (artefacto no encontrado)\\033[0m\\n'; \
                 xdg-open {dir} 2>/dev/null || true; \
               fi; \
             else \
               echo; printf '\\033[31m[ERROR] El build fallo.\\033[0m\\n'; \
             fi; \
             echo; printf '\\033[33mPresiona Enter para cerrar...\\033[0m'; read -r",
            command = command,
            rename_snippet = rename_snippet,
            find = sh_newest_artifact(folder, exts),
            dir = dir,
        )
    }
}

/// Elimina un directorio recursivamente manejando rutas largas en Windows.
/// En Windows usa PowerShell (Remove-Item -LiteralPath -Recurse -Force)
/// que soporta rutas de más de 260 caracteres.
pub fn remove_dir(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let ps = format!(
            "Remove-Item -LiteralPath '{}' -Recurse -Force",
            path.display().to_string().replace('\'', "''")
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .with_context(|| format!("Falló Remove-Item en: {}", path.display()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Error al eliminar {}:\n{}", path.display(), stderr);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("Error al eliminar {}", path.display()))
    }
}

pub fn clear_screen() {
    #[cfg(windows)]
    {
        let _ = Command::new("cmd").args(["/c", "cls"]).status();
    }
    #[cfg(not(windows))]
    {
        print!("\x1B[2J\x1B[1;1H");
    }
}
