use std::process::{Command, Output};
use anyhow::{Context, Result};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

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

pub fn spawn_new_window(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    let ps_command = format!(
        "$host.UI.RawUI.WindowTitle = '{}'; Set-Location '{}'; {}",
        title,
        dir.display(),
        command
    );
    #[cfg(windows)]
    {
        Command::new("powershell")
            .creation_flags(CREATE_NEW_CONSOLE)
            .args(["-NoExit", "-Command", &ps_command])
            .spawn()
            .context("No se pudo abrir nueva ventana de PowerShell")?;
    }
    #[cfg(not(windows))]
    {
        Command::new("powershell")
            .args(["-NoExit", "-Command", &ps_command])
            .spawn()
            .context("No se pudo abrir nueva ventana de PowerShell")?;
    }
    Ok(())
}

/// Igual que spawn_new_window pero sin -NoExit (la ventana se cierra al terminar).
pub fn spawn_new_window_autoexit(title: &str, dir: &std::path::Path, command: &str) -> Result<()> {
    let ps_command = format!(
        "$host.UI.RawUI.WindowTitle = '{}'; Set-Location '{}'; {}",
        title,
        dir.display(),
        command
    );
    #[cfg(windows)]
    {
        Command::new("powershell")
            .creation_flags(CREATE_NEW_CONSOLE)
            .args(["-Command", &ps_command])
            .spawn()
            .context("No se pudo abrir nueva ventana de PowerShell")?;
    }
    #[cfg(not(windows))]
    {
        Command::new("powershell")
            .args(["-Command", &ps_command])
            .spawn()
            .context("No se pudo abrir nueva ventana de PowerShell")?;
    }
    Ok(())
}

/// Abre una nueva ventana CMD con stdin funcional para comandos interactivos
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
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .arg("-c")
            .arg(format!("{}; echo 'Presiona Enter para cerrar...'; read", command))
            .spawn()
            .context("No se pudo abrir nueva terminal")?;
    }
    Ok(())
}

/// Envuelve un comando de build para que, al terminar con éxito, abra el
/// Explorador de Windows resaltando el artefacto más reciente (setup.exe, APK,
/// AAB) dentro de `folder` y cierre la ventana. Si el build falla, muestra un
/// mensaje y espera una tecla (la ventana no se cierra sola para poder leer el error).
pub fn reveal_on_success(command: &str, folder: &std::path::Path, exts: &[&str]) -> String {
    let folder_ps = folder.display().to_string().replace('\'', "''");
    let ext_list = exts
        .iter()
        .map(|e| format!("'.{}'", e))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}; if ($?) {{ $f = Get-ChildItem -LiteralPath '{}' -Recurse -File | Where-Object {{ $_.Extension -in {} }} | Sort-Object LastWriteTime -Descending | Select-Object -First 1; if ($f) {{ explorer.exe (\"/select,\" + $f.FullName) }}; exit }} else {{ Write-Host ''; Write-Host '[ERROR] El build fallo. Presiona Enter para cerrar...' -ForegroundColor Red; Read-Host; exit }}",
        command, folder_ps, ext_list
    )
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
