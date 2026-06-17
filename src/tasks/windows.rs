use crate::utils::{cmd, context::ProjectContext};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner().template("{spinner} {msg}")?);
    pb.set_message("Iniciando desarrollo en Windows...");

    pb.finish_and_clear();
    println!("\n[INFO] Iniciando desarrollo en Windows...");
    println!("[INFO] Servidor: http://localhost:1420\n");

    cmd::spawn_new_window(
        "Tauri Windows (Dev)",
        &ctx.script_dir,
        "npm run tauri dev",
    )?;

    Ok(())
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Windows (Release)...\n");

    cmd::spawn_new_window(
        "Windows Build (Release)",
        &ctx.script_dir,
        "npm run tauri build",
    )?;

    pause();
    Ok(())
}

fn pause() {
    use std::io::{self, Write};
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
