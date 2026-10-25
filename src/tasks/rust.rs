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
    // El binario de release adopta el nombre real + versión: "compass v0.1.0".
    let rename = read_cargo_app_meta(ctx).map(|(name, ver)| format!("{} v{}", name, ver));
    cmd::run_build_and_reveal(
        "cargo build --release",
        &ctx.script_dir,
        &release_dir,
        crate::utils::platform::desktop_artifact_exts(),
        rename.as_deref(),
    )?;
    Ok(())
}

/// Lee `name` y `version` de `Cargo.toml` (sección `[package]`).
fn read_cargo_app_meta(ctx: &ProjectContext) -> Option<(String, String)> {
    let content = std::fs::read_to_string(ctx.script_dir.join("Cargo.toml")).ok()?;
    let mut name = None;
    let mut version = None;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim();
            let val = v.trim().trim_matches('"').to_string();
            if key == "name" {
                name = Some(val);
            } else if key == "version" {
                version = Some(val);
            }
        }
    }
    Some((name?, version?))
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Ejecutando 'cargo clean'...");
    cmd::run_in_dir_status("cargo", &["clean"], &ctx.script_dir)?;
    println!("[OK] 'cargo clean' completado.");
    Ok(())
}
