use anyhow::Result;
use crate::utils::cmd;
use crate::utils::context::ProjectContext;

pub fn run(ctx: &ProjectContext) -> Result<()> {
    cmd::run_in_current_window("cargo run", &ctx.script_dir)?;
    Ok(())
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    // Sin extensión en Linux: `desktop_artifact_exts()` lo resuelve por plataforma.
    let release_dir = ctx.script_dir.join("target/release");
    cmd::run_build_and_reveal(
        "cargo build --release",
        &ctx.script_dir,
        &release_dir,
        crate::utils::platform::desktop_artifact_exts(),
        None,
    )?;
    Ok(())
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Ejecutando 'cargo clean'...");
    cmd::run_in_dir_status("cargo", &["clean"], &ctx.script_dir)?;
    println!("[OK] 'cargo clean' completado.");
    Ok(())
}
