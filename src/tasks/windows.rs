use crate::utils::{cmd, context::ProjectContext};
use anyhow::Result;

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    if !ctx.has_node_modules {
        println!("[INFO] node_modules no encontrado. Ejecutando npm install...");
        cmd::run_npm_install(&ctx.script_dir)?;
    }

    println!("\n[INFO] Iniciando desarrollo de escritorio...");
    println!("[INFO] Servidor: http://localhost:1420\n");

    cmd::spawn_new_window_interactive(
        "Tauri Desktop (Dev)",
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

    println!("\n[INFO] Compilando aplicación de escritorio (Release)...\n");

    let bundle_dir = ctx.script_dir.join("src-tauri/target/release/bundle");
    // En Linux `tauri build` produce AppImage/deb, no exe/msi; el helper de
    // plataforma devuelve la lista correcta y, si viene vacía, `find` no filtra
    // por extensión y revela igual.
    let cmd_line = cmd::reveal_on_success(
        "npm run tauri build",
        &bundle_dir,
        crate::utils::platform::desktop_artifact_exts(),
        None,
    );

    cmd::spawn_new_window(
        "Desktop Build (Release)",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}
