use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;
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

/// Ejecuta `command` en la **ventana actual** (heredando stdin/stdout/stderr) y
/// espera a que termine.
///
/// Los builds y procesos largos corren aquí en vez de abrir una segunda ventana:
/// esa segunda ventana se abría con PowerShell 5.1 (~2-3 s de arranque, más con
/// el perfil de PSReadLine predictivo) y era el último freno a la apertura de las
/// tarjetas. La ventana actual la abre el TUI con `cmd`, que es instantánea.
///
/// `envs` inyecta variables de entorno (p. ej. `RELEASE_*` para firmar).
pub fn run_build_and_reveal(
    command: &str,
    dir: &std::path::Path,
    folder: &std::path::Path,
    exts: &[&str],
    rename_to: Option<&str>,
) -> Result<()> {
    run_build_and_reveal_with_env(command, dir, folder, exts, rename_to, &[])
}

/// Igual que [`run_build_and_reveal`] pero con variables de entorno extra.
pub fn run_build_and_reveal_with_env(
    command: &str,
    dir: &std::path::Path,
    folder: &std::path::Path,
    exts: &[&str],
    rename_to: Option<&str>,
    envs: &[(String, String)],
) -> Result<()> {
    let _ = run_build_and_reveal_with_env_outcome(command, dir, folder, exts, rename_to, envs)?;
    Ok(())
}

/// Resultado de un build: si tuvo éxito y dónde quedó el artefacto.
///
/// El path es el **ya renombrado** (`<base>.<ext>`), que es el que se instala
/// en el dispositivo: si se renombra después, se instalaría el fichero viejo.
pub struct BuildOutcome {
    pub success: bool,
    pub artifact: Option<PathBuf>,
}

/// Igual que [`run_build_and_reveal_with_env`] pero devuelve el resultado del
/// build y la ruta del artefacto, para poder hacer algo con él después
/// (p. ej. instalarlo en el teléfono).
pub fn run_build_and_reveal_with_env_outcome(
    command: &str,
    dir: &std::path::Path,
    folder: &std::path::Path,
    exts: &[&str],
    rename_to: Option<&str>,
    envs: &[(String, String)],
) -> Result<BuildOutcome> {
    run_build_and_reveal_filtered(command, dir, folder, exts, rename_to, envs, None)
}

/// Igual que [`run_build_and_reveal`] pero sólo acepta artefactos cuyo nombre
/// contenga alguno de `name_contains`.
///
/// Hace falta cuando el build deja **varios** ejecutables en la misma carpeta y
/// el "más reciente" no es el entregable: `wails3 task package` deja en `bin/`
/// tanto `<app>.exe` (el portable, que compila como dependencia) como
/// `<app>-amd64-installer.exe` (el instalador de NSIS). Sin el filtro se
/// revelaba el portable y el instalador quedaba escondido al lado.
pub fn run_build_and_reveal_named(
    command: &str,
    dir: &std::path::Path,
    folder: &std::path::Path,
    exts: &[&str],
    name_contains: &[&str],
    rename_to: Option<&str>,
) -> Result<()> {
    let _ = run_build_and_reveal_filtered(
        command,
        dir,
        folder,
        exts,
        rename_to,
        &[],
        Some(name_contains),
    )?;
    Ok(())
}

fn run_build_and_reveal_filtered(
    command: &str,
    dir: &std::path::Path,
    folder: &std::path::Path,
    exts: &[&str],
    rename_to: Option<&str>,
    envs: &[(String, String)],
    name_contains: Option<&[&str]>,
) -> Result<BuildOutcome> {
    println!();
    println!("[INFO] Ejecutando: {command}");
    println!();
    let success = run_shell_status(command, dir, envs)?;
    let artifact = if success {
        reveal_artifact_filtered(folder, exts, rename_to, name_contains)
    } else {
        println!();
        println!("[ERROR] El build fallo.");
        None
    };
    println!();
    if success {
        crate::utils::ui::pause_for(10);
    } else {
        // En caso de error, esperar a que el usuario presione Enter para que
        // pueda leer los errores antes de que la ventana se cierre.
        crate::utils::ui::pause();
    }
    Ok(BuildOutcome { success, artifact })
}

/// Ejecuta un programa **sin shell**, con argumentos, variables de entorno y
/// `dir` como directorio de trabajo. Devuelve si tuvo éxito.
///
/// Es la alternativa a `run_shell_status` cuando hay rutas con espacios o
/// flags con comillas (`-ldflags="-w -s"`): el shell de Windows (`cmd /c`) no
/// las respeta, y `cmd.exe` además tiene su propia regla de comillas (borra
/// la primera y la última) que rompe los argumentos.
///
/// Con `Command::new(program).args(args)` cada argumento llega tal cual, sin
/// que nadie lo reinterprete.
pub fn run_program(
    program: &str,
    args: &[&str],
    dir: &std::path::Path,
    envs: &[(String, String)],
) -> Result<bool> {
    let mut c = Command::new(program);
    c.args(args);
    c.current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    Ok(c.status()
        .with_context(|| format!("No se pudo ejecutar: {program}"))?
        .success())
}

/// Igual que [`run_program`] pero **capturando** la salida, para poder
/// inspeccionarla.
///
/// Sirve para pasos cuya salida hay que leer (por ejemplo, comprobar que el
/// generador de bindings ha cargado los paquetes de Go y no ha generado cero
/// servicios).
pub fn run_program_capture(
    program: &str,
    args: &[&str],
    dir: &std::path::Path,
    envs: &[(String, String)],
) -> Result<Output> {
    let mut c = Command::new(program);
    c.args(args);
    c.current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    c.output()
        .with_context(|| format!("No se pudo ejecutar: {program}"))
}

/// Ejecuta un comando largo (p. ej. `cargo run`) en esta ventana y la deja
/// abierta hasta que el usuario pulse Enter.
pub fn run_in_current_window(command: &str, dir: &std::path::Path) -> Result<()> {
    println!();
    println!("[INFO] Ejecutando: {command}");
    println!();
    if !run_shell_status(command, dir, &[])? {
        println!();
        println!("[ERROR] El comando fallo.");
    }
    println!();
    crate::utils::ui::pause();
    Ok(())
}

/// Lanza `command` en un shell (`cmd /c` en Windows, `sh -lc` en Linux)
/// heredando stdio, con `dir` como directorio de trabajo.
fn shell_status(
    command: &str,
    dir: &std::path::Path,
    envs: &[(String, String)],
) -> Result<std::process::ExitStatus> {
    #[cfg(windows)]
    let mut c = {
        let mut c = Command::new("cmd");
        c.arg("/c").arg(command);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let mut c = Command::new("sh");
        c.arg("-lc").arg(command);
        c
    };
    c.current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    c.status()
        .with_context(|| format!("No se pudo ejecutar: {command}"))
}

/// Ejecuta `command` en un shell heredando stdio y devuelve si tuvo éxito.
///
/// Se usa fuera de `cmd.rs` para secuencias de más de un paso (p. ej. instalar
/// el APK de debug y, solo si eso funcionó, lanzarlo con `am start`).
pub fn run_shell_status(
    command: &str,
    dir: &Path,
    envs: &[(String, String)],
) -> Result<bool> {
    Ok(shell_status(command, dir, envs)?.success())
}

/// Abre una ventana de terminal que **se cierra** al terminar el comando.
///
/// En Windows usa `cmd /c`, **no** PowerShell: PowerShell 5.1 tarda ~2-3 s en
/// arrancar (y más con un perfil de PSReadLine predictivo, que instala la propia
/// tarjeta "Terminal"), lo que se veía como una consola en blanco durante unos
/// segundos en las tarjetas interactivas (Git, Signing, Clean). `cmd` arranca en
/// milisegundos y el comando es una simple invocación de `compass.exe`, así que
/// no se necesita nada de PowerShell.
pub fn spawn_new_window_autoexit(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    #[cfg(windows)]
    {
        // `raw_arg` y no `.args`: `.args` reescapa las comillas dobles del
        // comando (`"C:\...\compass.exe"` → `\"C:\...\compass.exe\"`), y cmd no
        // entiende `\"`, así que interpretaría el `\` como parte de la ruta.
        let cmd_line = format!("/c title {} & {}", title, command);
        Command::new("cmd")
            .creation_flags(CREATE_NEW_CONSOLE)
            .current_dir(dir)
            .raw_arg(&cmd_line)
            .spawn()
            .context("No se pudo abrir nueva ventana de CMD")?;
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
        // `raw_arg` por el mismo motivo que en `spawn_new_window_autoexit`: si el
        // comando lleva comillas dobles (rutas con espacios), `.args` las
        // reescaparía a `\"` y cmd no las entendería.
        let cmd_line = format!("/c title {} & {} & pause", title, command);
        Command::new("cmd")
            .creation_flags(CREATE_NEW_CONSOLE)
            .current_dir(dir)
            .raw_arg(&cmd_line)
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

/// Busca el archivo más reciente dentro de `folder` (recursivo) cuya extensión
/// esté en `exts`. `exts` vacío significa "cualquier archivo".
///
/// Sólo lo usan los tests: el resto del código pasa por
/// [`newest_artifact_filtered`], que además puede filtrar por nombre.
#[cfg(test)]
fn newest_artifact(folder: &Path, exts: &[&str]) -> Option<PathBuf> {
    newest_artifact_filtered(folder, exts, None)
}

/// Igual que [`newest_artifact`] pero, si `name_contains` viene, el nombre del
/// archivo tiene que contener **alguno** de esos fragmentos (sin distinguir
/// mayúsculas).
fn newest_artifact_filtered(
    folder: &Path,
    exts: &[&str],
    name_contains: Option<&[&str]>,
) -> Option<PathBuf> {
    fn walk(
        dir: &Path,
        exts: &[&str],
        name_contains: Option<&[&str]>,
        best: &mut Option<(SystemTime, PathBuf)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, exts, name_contains, best);
                continue;
            }
            if !exts.is_empty() {
                let matches = path
                    .extension()
                    .map(|ext| exts.iter().any(|e| ext.eq_ignore_ascii_case(e)))
                    .unwrap_or(false);
                if !matches {
                    continue;
                }
            }
            if let Some(fragments) = name_contains {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !fragments
                    .iter()
                    .any(|f| name.contains(&f.to_lowercase()))
                {
                    continue;
                }
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    if best.as_ref().map_or(true, |(best_t, _)| mtime > *best_t) {
                        *best = Some((mtime, path));
                    }
                }
            }
        }
    }

    if !folder.is_dir() {
        return None;
    }
    let mut best = None;
    walk(folder, exts, name_contains, &mut best);
    best.map(|(_, p)| p)
}

/// Renombra `path` a `<base>.<ext>` (conservando la extensión) y devuelve el
/// nuevo path, o `None` si falla.
fn rename_artifact(path: &Path, base: &str) -> Option<PathBuf> {
    let ext = path.extension()?.to_string_lossy().into_owned();
    let target = path.with_file_name(format!("{base}.{ext}"));
    if target == path {
        return Some(path.to_path_buf());
    }
    std::fs::rename(path, &target).ok()?;
    Some(target)
}

/// Muestra el resultado de un build exitoso y abre el explorador sobre el
/// artefacto más reciente de `folder` (o sobre la carpeta si no encuentra nada).
///
/// Devuelve el path del artefacto (ya renombrado), si lo hubo.
///
/// `name_contains` filtra por fragmentos del nombre; `None` acepta cualquiera.
fn reveal_artifact_filtered(
    folder: &Path,
    exts: &[&str],
    rename_to: Option<&str>,
    name_contains: Option<&[&str]>,
) -> Option<PathBuf> {
    let artifact = newest_artifact_filtered(folder, exts, name_contains);
    match artifact {
        Some(path) => {
            let path = rename_to
                .and_then(|base| rename_artifact(&path, base))
                .unwrap_or(path);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            println!();
            println!("[OK] Build exitoso: {name}");
            println!("[INFO] Artefacto: {}", path.display());
            open_in_explorer(&path);
            Some(path)
        }
        None => {
            println!();
            println!("[OK] Build exitoso (artefacto no encontrado en la carpeta)");
            open_folder(folder);
            None
        }
    }
}

/// Convierte un path a su forma **Windows** para el Explorer: separadores
/// backslash puros.
///
/// Los literales de ruta del crate usan `/` (portabilidad Linux/Windows) y los
/// `join("a/b/c")` los conservan, así que un path construido por aquí puede
/// llevar separadores mezclados (`D:\P\src-tauri/gen/android/...`). `explorer
/// /select` no parsea ese caso: ignora el argumento y abre el Escritorio (o
/// Documentos). Con backslashes puros sí funciona.
#[cfg(windows)]
fn windows_path_for_explorer(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

/// Abre el explorador resaltando `path`.
fn open_in_explorer(path: &Path) {
    #[cfg(windows)]
    {
        let p = windows_path_for_explorer(path);
        // `raw_arg`: el parser de Explorer necesita `/select,"<ruta>"`, con las
        // comillas dobles alrededor de la ruta. Con `.args` Rust citaría el
        // argumento entero y Explorer recibiría `/select,"ruta"` mal formado, lo
        // que abre Documentos en vez de la carpeta.
        let _ = Command::new("explorer")
            .raw_arg(format!("/select,\"{p}\""))
            .spawn();
    }
    #[cfg(not(windows))]
    {
        // Abrir el archivo lanzaría la app o un visor inesperado; la carpeta
        // contenedora es lo que "revelar" significa de verdad.
        let target = path.parent().unwrap_or(path);
        let _ = Command::new("xdg-open").arg(target).spawn();
    }
}

/// Abre una carpeta en el explorador.
fn open_folder(folder: &Path) {
    #[cfg(windows)]
    {
        // Mismo motivo que en `open_in_explorer`: `/` mezcladas → Escritorio.
        let p = windows_path_for_explorer(folder);
        let _ = Command::new("explorer").arg(&p).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(folder).spawn();
    }
}

/// Pone el título de la consola con el **nombre del directorio del proyecto**.
///
/// Cmd cambia el título de la ventana al *command line* del hijo mientras corre
/// (por eso la ventana mostraba `C:\Users\Admin\.cargo\bin\compass.exe …`); si es
/// el propio proceso quien lo fija al arrancar, se sobreescribe al abrirse.
pub fn set_console_title(title: &str) {
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle(title.to_string()));
}

/// Elimina un directorio recursivamente manejando rutas largas en Windows.
///
/// `std::fs::remove_dir_all` es instantáneo y cubre la inmensa mayoría de los
/// casos; PowerShell se reserva para lo único que no puede: rutas de más de 260
/// caracteres. Llamar a PowerShell **siempre** costaba ~2-3 s por directorio, y
/// una limpieza completa lanza una decena de ellos.
pub fn remove_dir(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        if std::fs::remove_dir_all(path).is_ok() {
            return Ok(());
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("compass-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path, when: SystemTime) {
        std::fs::write(path, b"x").unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    #[test]
    fn newest_artifact_filters_by_extension_and_picks_most_recent() {
        let dir = temp_dir("newest");
        let now = SystemTime::now();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let exe = dir.join("old.exe");
        let apk = sub.join("new.apk");
        let txt = sub.join("skip.txt");
        touch(&exe, now - Duration::from_secs(30));
        touch(&apk, now);
        touch(&txt, now + Duration::from_secs(5));

        assert_eq!(newest_artifact(&dir, &["apk"]), Some(apk.clone()));
        assert_eq!(newest_artifact(&dir, &["exe"]), Some(exe.clone()));
        assert_eq!(newest_artifact(&dir, &["msi"]), None);
        // Sin filtro: el más reciente entre todos los archivos (incluido el .txt).
        assert_eq!(newest_artifact(&dir, &[]), Some(txt.clone()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El build del instalador de Wails deja en `bin/` el binario portable y el
    /// instalador. El filtro por nombre tiene que quedarse con el instalador
    /// aunque el portable sea más reciente.
    #[test]
    fn newest_artifact_filter_by_name_ignores_the_portable_binary() {
        let dir = temp_dir("installer");
        let now = SystemTime::now();
        let portable = dir.join("school-tracker.exe");
        let installer = dir.join("school-tracker-amd64-installer.exe");
        touch(&portable, now);
        touch(&installer, now - Duration::from_secs(60));

        assert_eq!(
            newest_artifact_filtered(&dir, &["exe", "msix"], Some(&["installer"])),
            Some(installer.clone()),
            "sin el filtro se revelaría el portable"
        );

        let msix = dir.join("school-tracker-amd64.msix");
        touch(&msix, now + Duration::from_secs(10));
        assert_eq!(
            newest_artifact_filtered(&dir, &["exe", "msix"], Some(&["installer", "msix"])),
            Some(msix.clone()),
            "los formatos empaquetados se aceptan por extensión"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newest_artifact_missing_folder_is_none() {
        assert_eq!(
            newest_artifact(Path::new("no-existe-compass"), &["apk"]),
            None
        );
    }

    #[test]
    fn rename_artifact_keeps_extension() {
        let dir = temp_dir("rename");
        let path = dir.join("app-v1.apk");
        std::fs::write(&path, b"x").unwrap();
        let renamed = rename_artifact(&path, "App-v2.0.0").unwrap();
        assert_eq!(renamed, dir.join("App-v2.0.0.apk"));
        assert!(renamed.exists());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regresión: los `join("a/b/c")` conservan las `/` del literal y `explorer
    /// /select` abre el Escritorio en vez de seleccionar con un path de
    /// separadores mezclados. El path que llega al Explorer debe ser puro `\`.
    #[cfg(windows)]
    #[test]
    fn path_for_explorer_normalizes_separators() {
        let path = Path::new(r"D:\P\src-tauri/gen/android/app/build/outputs/apk/universal/release/app-release.apk");
        let p = windows_path_for_explorer(path);
        assert_eq!(
            p,
            r"D:\P\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-release.apk"
        );
        assert!(!p.contains('/'));
    }
}
