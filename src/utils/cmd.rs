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
    let status = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("Falló al ejecutar: {} {} en {:?}", cmd, args.join(" "), dir))?;
    if !status.success() {
        anyhow::bail!("Comando falló con código: {}", status);
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
