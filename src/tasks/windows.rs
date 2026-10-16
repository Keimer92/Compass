use crate::utils::{cmd, context::ProjectContext, tauri};
use anyhow::Result;

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    tauri::ensure(ctx)?;

    println!("\n[INFO] Iniciando desarrollo de escritorio...");
    println!("[INFO] Servidor: http://localhost:1420\n");

    cmd::spawn_new_window_interactive(
        "Tauri Desktop (Dev)",
        &ctx.script_dir,
        &tauri::command(ctx, "dev"),
    )?;

    Ok(())
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    tauri::ensure(ctx)?;

    println!("\n[INFO] Compilando aplicación de escritorio (Release)...\n");

    let bundle_dir = ctx.script_dir.join("src-tauri/target/release/bundle");
    // En Linux `tauri build` produce AppImage/deb, no exe/msi; el helper de
    // plataforma devuelve la lista correcta y, si viene vacía, no se filtra por
    // extensión y revela igual.
    cmd::run_build_and_reveal(
        &tauri::command(ctx, "build"),
        &ctx.script_dir,
        &bundle_dir,
        crate::utils::platform::desktop_artifact_exts(),
        None,
    )?;

    Ok(())
}
