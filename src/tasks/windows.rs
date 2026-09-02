use crate::utils::{cmd, context::ProjectContext};
use anyhow::Result;

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    if !ctx.has_node_modules {
        println!("[INFO] node_modules no encontrado. Ejecutando npm install...");
        cmd::run_npm_install(&ctx.script_dir)?;
    }

    println!("\n[INFO] Iniciando desarrollo en Windows...");
    println!("[INFO] Servidor: http://localhost:1420\n");

    cmd::spawn_new_window_interactive(
        "Tauri Windows (Dev)",
        &ctx.script_dir,
        "npm run tauri dev",
    )?;

    Ok(())
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    if !ctx.has_node_modules {
        println!("[INFO] node_modules no encontrado. Ejecutando npm install...");
        cmd::run_npm_install(&ctx.script_dir)?;
    }

    println!("\n[INFO] Compilando Windows (Release)...\n");

    let bundle_dir = ctx.script_dir.join("src-tauri\\target\\release\\bundle");
    let cmd_line = cmd::reveal_on_success("npm run tauri build", &bundle_dir, &["exe", "msi"], None);

    cmd::spawn_new_window(
        "Windows Build (Release)",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}
