use crate::utils::cmd;
use crate::utils::context::ProjectContext;
use anyhow::Result;
use std::path::PathBuf;

pub fn run(ctx: &ProjectContext) -> Result<()> {
    cmd::run_in_current_window("cargo run", &ctx.script_dir)?;
    Ok(())
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
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

/// Compila un release de Rust para **una** arquitectura de Windows.
///
/// El artefacto queda en `target/<triple>/release` y se renombra a
/// `<name> v<version> [label].exe`; `label` vacío equivale a x64.
pub fn build_arch(ctx: &ProjectContext, triple: &str, label: &str) -> Result<PathBuf> {
    let _ = cmd::run_program("rustup", &["target", "add", triple], &ctx.script_dir, &[]);

    let display = if label.is_empty() { "x64" } else { label };
    println!("\n[INFO] Compilando Rust para {display} ({triple})...\n");
    if !cmd::run_shell_status(
        &format!("cargo build --release --target {triple}"),
        &ctx.script_dir,
        &[],
    )? {
        anyhow::bail!(
            "[ERROR] `cargo build --release --target {triple}` fallo. \
             En ARM64 suele faltar el enlazador de Visual Studio para ARM64."
        );
    }

    let dir = ctx.script_dir.join("target").join(triple).join("release");
    let artifact = cmd::newest_file_in(&dir, crate::utils::platform::desktop_artifact_exts())
        .ok_or_else(|| anyhow::anyhow!("[ERROR] No se encontro el .exe en {}", dir.display()))?;
    let base = match read_cargo_app_meta(ctx) {
        Some((name, version)) if label.is_empty() => format!("{name} v{version}"),
        Some((name, version)) => format!("{name} v{version} {label}"),
        None if label.is_empty() => "app".to_string(),
        None => format!("app {label}"),
    };
    cmd::rename_artifact(&artifact, &base)
        .ok_or_else(|| anyhow::anyhow!("[ERROR] No se pudo renombrar {}", artifact.display()))
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
