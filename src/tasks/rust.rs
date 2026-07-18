use anyhow::Result;
use crate::utils::cmd;
use crate::utils::context::ProjectContext;

pub fn run(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Iniciando 'cargo run'...");
    cmd::spawn_new_window("cargo run", &ctx.script_dir, "cargo run")?;
    println!("[OK] 'cargo run' lanzado en nueva ventana.");
    Ok(())
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Ejecutando 'cargo build --release'...");
    cmd::spawn_new_window("cargo build", &ctx.script_dir, "cargo build --release")?;
    println!("[OK] 'cargo build --release' lanzado en nueva ventana.");
    Ok(())
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Ejecutando 'cargo clean'...");
    cmd::run_in_dir_status("cargo", &["clean"], &ctx.script_dir)?;
    println!("[OK] 'cargo clean' completado.");
    Ok(())
}
