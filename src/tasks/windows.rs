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
    cmd::run_build_and_reveal(
        &tauri::command(ctx, "build"),
        &ctx.script_dir,
        &bundle_dir,
        crate::utils::platform::desktop_artifact_exts(),
        read_tauri_app_meta(ctx)
            .map(|(n, v)| format!("{} v{}", n, v))
            .as_deref(),
    )?;

    Ok(())
}

/// Lee `productName` y `version` de `src-tauri/tauri.conf.json`.
fn read_tauri_app_meta(ctx: &ProjectContext) -> Option<(String, String)> {
    let conf_path = ctx.script_dir.join("src-tauri/tauri.conf.json");
    let conf_str = std::fs::read_to_string(&conf_path).ok()?;
    let conf: serde_json::Value = serde_json::from_str(&conf_str).ok()?;
    let name = conf["productName"].as_str()?;
    let version = conf["version"].as_str()?;
    Some((name.to_string(), version.to_string()))
}
