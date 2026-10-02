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

/// Emulador de terminal + prefijo de args para fijar título y ejecutar `sh -lc`.
///
/// El último elemento de cada tupla es el argv que precede al script de shell.
/// `cosmic-term` va primero: es el terminal por defecto de Pop!_OS/COSMIC, y en
/// una instalación COSMIC limpia puede ser el único emulador disponible.
///
/// Nota: `cosmic-term` sigue la convención de `alacritty_terminal` (`-e` para
/// ejecutar, `--title` para el título). Si una versión rechazara `-e`, `spawn()`
/// devuelve error y se reporta en vez de fallar en silencio.
#[cfg(not(windows))]
const TERMINALS: &[(&str, &[&str])] = &[
    ("cosmic-term", &["--title"]),
    ("gnome-terminal", &["--title"]),
    ("konsole", &["--title"]),
    ("xfce4-terminal", &["--title"]),
    ("alacritty", &["--title"]),
    ("kitty", &["--title"]),
    ("wezterm", &["start", "--"]),
    ("xterm", &["-T"]),
];

/// Devuelve el primer emulador de terminal disponible en el PATH.
#[cfg(not(windows))]
fn detect_terminal() -> Option<(&'static str, &'static [&'static str])> {
    TERMINALS.iter().copied().find(|(bin, _)| {
        Command::new("sh")
            .args(["-c", &format!("command -v {bin}")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Escapa una cadena para incrustarla entre comillas simples en POSIX `sh`.
#[cfg(not(windows))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Envuelve `script` en un emulador de terminal nuevo y lo lanza.
#[cfg(not(windows))]
fn spawn_in_terminal(title: &str, script: &str) -> Result<()> {
    let (bin, title_flag) = detect_terminal().with_context(|| {
        "[ERROR] No se encontró ningún emulador de terminal.\n\
         Instala uno de: gnome-terminal, konsole, xfce4-terminal, alacritty, kitty, wezterm, xterm"
    })?;

    // `wezterm start -- <cmd>` no acepta `--title`; se omite el flag.
    let mut args: Vec<String> = title_flag.iter().map(|s| s.to_string()).collect();
    if !title_flag.is_empty() {
        args.push(title.to_string());
    }
    args.push("-e".to_string());
    args.push("sh".to_string());
    args.push("-lc".to_string());
    args.push(script.to_string());

    Command::new(bin)
        .args(&args)
        .spawn()
        .with_context(|| format!("No se pudo abrir una ventana de {bin}"))?;
    Ok(())
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
