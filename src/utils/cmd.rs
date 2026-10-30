use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::SystemTime;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

// ── Shell: PowerShell 7 con fallback a Windows PowerShell 5.1 ─────────────

/// Shell con el que se ejecuta todo lo que necesita un intérprete.
///
/// Se prefiere `pwsh`: es el que aporta el icono nuevo a las ventanas que abre
/// Compass. Se comprueba **ejecutándolo**, no buscándolo con `where`: el alias
/// de Microsoft Store es un stub de 0 bytes que `where` encuentra y que no
/// arranca. Sin pwsh se usa Windows PowerShell 5.1, que siempre existe.
pub fn shell() -> &'static str {
    static SHELL: OnceLock<&'static str> = OnceLock::new();
    SHELL.get_or_init(|| {
        let disponible = Command::new("pwsh")
            .args(["-NoLogo", "-NoProfile", "-Command", "exit 0"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if disponible {
            "pwsh"
        } else {
            "powershell"
        }
    })
}

/// Ruta completa del shell activo, para las entradas del registro.
///
/// Explorer no hereda el PATH de la terminal desde la que se instaló Compass,
/// así que el valor debe apuntar al ejecutable, no al nombre.
pub fn shell_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        if shell() == "pwsh" {
            if let Ok(program_files) = std::env::var("ProgramFiles") {
                let candidate = Path::new(&program_files)
                    .join("PowerShell")
                    .join("7")
                    .join("pwsh.exe");
                if candidate.is_file() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
            if let Ok(out) = Command::new("where.exe").arg("pwsh").output() {
                if let Some(line) = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                {
                    return line.to_string();
                }
            }
            "pwsh.exe".to_string()
        } else {
            std::env::var("SystemRoot")
                .map(|root| format!("{root}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
                .unwrap_or_else(|_| "powershell.exe".to_string())
        }
    })
}

/// Literal de cadena de PowerShell, con las comillas simples duplicadas.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// `true` cuando este proceso corre dentro de una pestaña del contenedor de
/// Compass (ver `container.rs`), no en una ventana propia.
///
/// La marca la pone el contenedor al abrir la sesión ConPTY. Con ella, los
/// lanzadores de abajo ejecutan el comando en la consola actual en vez de
/// abrir otra ventana: la pestaña **es** la consola del comando.
pub fn embedded() -> bool {
    std::env::var_os("COMPASS_EMBEDDED").is_some()
}

/// Argumentos con los que se lanza un script de PowerShell.
fn window_args(script: &str) -> Vec<&str> {
    vec!["-NoLogo", "-NoProfile", "-Command", script]
}

/// Script de una ventana nueva: título y comando.
///
/// El `&` (operador de llamada) es obligatorio: los comandos que empiezan por
/// una ruta entre comillas (`"C:\…\compass.exe" --task …`) se parsean como una
/// expresión de cadena y no se ejecutan sin él. El `Read-Host` sustituye al
/// `pause` de cmd para que la ventana no se cierre antes de leer el error.
fn window_script(title: &str, command: &str, interactive: bool) -> String {
    let mut script = format!("[Console]::Title = {}; & {}", ps_quote(title), command);
    if interactive {
        script.push_str("; Read-Host 'Presiona Enter para cerrar' | Out-Null");
    }
    script
}

/// Ejecuta un script de PowerShell con el shell activo y captura la salida.
pub fn run_ps(script: &str) -> Result<Output> {
    Command::new(shell())
        .args(window_args(script))
        .output()
        .with_context(|| format!("Falló al ejecutar PowerShell: {script}"))
}

/// Ejecuta un script de PowerShell en `dir`; error si termina con fallo.
pub fn run_ps_in_dir_status(script: &str, dir: &Path) -> Result<()> {
    if !dir.exists() {
        anyhow::bail!("El directorio no existe: {}", dir.display());
    }
    let status = Command::new(shell())
        .args(window_args(script))
        .current_dir(dir)
        .status()
        .with_context(|| format!("No se pudo ejecutar PowerShell: {script}"))?;
    if !status.success() {
        anyhow::bail!("El comando falló con código: {status}");
    }
    Ok(())
}

/// Igual que [`run_ps_in_dir_status`] pero devuelve si tuvo éxito.
pub fn run_ps_status(script: &str, dir: &Path, envs: &[(String, String)]) -> Result<bool> {
    let mut c = Command::new(shell());
    c.args(window_args(script));
    c.current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    Ok(c.status()
        .with_context(|| format!("No se pudo ejecutar PowerShell: {script}"))?
        .success())
}

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
        .with_context(|| {
            format!(
                "Falló al ejecutar '{}'. Verifica que el programa esté instalado y en el PATH.",
                cmd
            )
        })?;
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
        .with_context(|| {
            format!(
                "Falló al ejecutar '{}'. Verifica que el programa esté instalado y en el PATH.",
                cmd
            )
        })?;

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
///
/// Se llama a `npm.cmd` explícitamente: en PowerShell `npm` resuelve primero al
/// shim `npm.ps1`, que la ExecutionPolicy por defecto bloquea.
pub fn run_npm_install(dir: &std::path::Path) -> Result<()> {
    if !dir.exists() {
        anyhow::bail!("El directorio no existe: {}", dir.display());
    }
    if !dir.join("package.json").exists() {
        anyhow::bail!("No se encontró package.json en: {}", dir.display());
    }

    let ok = run_ps_status("npm.cmd install", dir, &[]).with_context(|| {
        format!(
            "No se pudo ejecutar npm. Verifica que Node.js esté instalado.\n\
             Ruta esperada del proyecto: {}",
            dir.display()
        )
    })?;
    if !ok {
        anyhow::bail!("npm install falló en: {}", dir.display());
    }
    Ok(())
}

/// Ejecuta `command` en la **ventana actual** (heredando stdin/stdout/stderr) y
/// espera a que termine.
///
/// Los builds y procesos largos corren aquí en vez de abrir una segunda ventana:
/// la ventana actual la abre el TUI con [`spawn_new_window_autoexit`], y una
/// segunda ventana sólo añadiría ruido. El comando se interpreta con el shell
/// activo (PowerShell), así que aquí no se codea sintaxis de cmd.
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

/// Lanza `command` con el shell activo, heredando stdio y con `dir` como
/// directorio de trabajo.
///
/// El `&` inicial permite que también funcionen los comandos que empiezan por
/// una ruta entrecomillada (adb con ruta absoluta, por ejemplo).
fn shell_status(
    command: &str,
    dir: &std::path::Path,
    envs: &[(String, String)],
) -> Result<std::process::ExitStatus> {
    let script = format!("& {command}");
    let mut c = Command::new(shell());
    c.args(window_args(&script));
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
pub fn run_shell_status(command: &str, dir: &Path, envs: &[(String, String)]) -> Result<bool> {
    Ok(shell_status(command, dir, envs)?.success())
}

/// Abre una ventana de terminal que **se cierra** al terminar el comando.
///
/// La aloja el shell activo (PowerShell 7 si está instalado): el icono de una
/// consola lo aporta el ejecutable que la crea, así que esto es lo que cambia
/// el icono de cmd por el de PowerShell. Se usa `-NoProfile` para que el
/// arranque sea determinista y para que PSReadLine no interfiera con el teclado
/// de los procesos hijos.
pub fn spawn_new_window_autoexit(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    if embedded() {
        let _ = shell_status(command, dir, &[]);
        return Ok(());
    }
    let script = window_script(title, command, false);
    Command::new(shell())
        .args(window_args(&script))
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(dir)
        .spawn()
        .context("No se pudo abrir una ventana de PowerShell")?;
    Ok(())
}

/// Abre una nueva terminal con stdin funcional para comandos interactivos
/// (flutter run, gradle run, etc.).
///
/// La ventana queda abierta tras el comando (`Read-Host`) para poder leer el
/// error si el proceso muere al arrancar.
pub fn spawn_new_window_interactive(
    title: &str,
    dir: &std::path::Path,
    command: &str,
) -> Result<()> {
    // En una pestaña del contenedor no hay que abrir nada: la consola actual
    // es la que el usuario está viendo. Se hereda stdio para que el comando
    // interactivo (flutter run, gradle run…) reciba el teclado de la pestaña.
    if embedded() {
        let _ = shell_status(command, dir, &[]);
        return Ok(());
    }
    let script = window_script(title, command, true);
    Command::new(shell())
        .args(window_args(&script))
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(dir)
        .spawn()
        .context("No se pudo abrir una ventana de PowerShell")?;
    Ok(())
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
                if !fragments.iter().any(|f| name.contains(&f.to_lowercase())) {
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
/// Los `join("a/b/c")` conservan `/`, así que un path construido por aquí puede
/// llevar separadores mezclados (`D:\P\src-tauri/gen/android/...`). `explorer
/// /select` no parsea ese caso: ignora el argumento y abre el Escritorio (o
/// Documentos). Con backslashes puros sí funciona.
fn windows_path_for_explorer(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

/// Abre el explorador resaltando `path`.
fn open_in_explorer(path: &Path) {
    let p = windows_path_for_explorer(path);
    // Explorer necesita `/select,"<ruta>"` con comillas embebidas.
    let _ = Command::new("explorer")
        .raw_arg(format!("/select,\"{p}\""))
        .spawn();
}

/// Abre una carpeta en el explorador.
fn open_folder(folder: &Path) {
    let p = windows_path_for_explorer(folder);
    let _ = Command::new("explorer").arg(&p).spawn();
}

/// Pone el título de la consola con el **nombre del directorio del proyecto**.
///
/// Cmd cambia el título de la ventana al *command line* del hijo mientras corre
/// (por eso la ventana mostraba `C:\Users\Admin\.cargo\bin\compass.exe …`); si es
/// el propio proceso quien lo fija al arrancar, se sobreescribe al abrirse.
pub fn set_console_title(title: &str) {
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::SetTitle(title.to_string())
    );
}

/// Elimina un directorio recursivamente manejando rutas largas en Windows.
///
/// `std::fs::remove_dir_all` es instantáneo y cubre la inmensa mayoría de los
/// casos; el shell se reserva para lo único que no puede: rutas de más de 260
/// caracteres. Lanzarlo **siempre** costaba el arranque de un proceso por
/// directorio, y una limpieza completa borra una decena.
pub fn remove_dir(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if std::fs::remove_dir_all(path).is_ok() {
        return Ok(());
    }
    let ps = format!(
        "Remove-Item -LiteralPath {} -Recurse -Force",
        ps_quote(&path.display().to_string())
    );
    let output = Command::new(shell())
        .args(["-NoLogo", "-NoProfile", "-Command", &ps])
        .output()
        .with_context(|| format!("Falló Remove-Item en: {}", path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al eliminar {}:\n{}", path.display(), stderr);
    }
    Ok(())
}

pub fn clear_screen() {
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    );
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
        let path = Path::new(
            r"D:\P\src-tauri/gen/android/app/build/outputs/apk/universal/release/app-release.apk",
        );
        let p = windows_path_for_explorer(path);
        assert_eq!(
            p,
            r"D:\P\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-release.apk"
        );
    }

    #[test]
    fn ps_quote_doubles_single_quotes() {
        assert_eq!(
            ps_quote("C:\\Proyectos\\Compass"),
            "'C:\\Proyectos\\Compass'"
        );
        assert_eq!(ps_quote("O'Brien"), "'O''Brien'");
    }

    /// El `&` es lo que permite ejecutar comandos que empiezan por una ruta
    /// entre comillas: sin él PowerShell lo parsea como una cadena y falla.
    #[test]
    fn window_script_uses_the_call_operator_and_no_cmd_syntax() {
        let script = window_script(
            "Compass - Proyecto",
            r#""C:\bin\compass.exe" --task run"#,
            false,
        );
        assert!(script.starts_with("[Console]::Title = 'Compass - Proyecto'; & "));
        assert!(script.contains(r#"& "C:\bin\compass.exe" --task run"#));
        assert!(!script.contains("title Compass"));
        assert!(!script.contains("/c "));
        assert!(!script.contains("& pause"));
    }

    #[test]
    fn interactive_windows_wait_for_enter_with_read_host() {
        let script = window_script("Dev", "flutter run -d windows", true);
        assert!(script.contains("& flutter run -d windows"));
        assert!(script.ends_with("Read-Host 'Presiona Enter para cerrar' | Out-Null"));
    }

    #[test]
    fn window_args_pin_the_non_interactive_profile() {
        let args = window_args("& cargo run");
        assert_eq!(
            args,
            vec!["-NoLogo", "-NoProfile", "-Command", "& cargo run"]
        );
    }

    #[test]
    fn shell_resolution_returns_a_runnable_name_and_path() {
        assert!(["pwsh", "powershell"].contains(&shell()));
        assert!(!shell_path().is_empty());
    }
}
