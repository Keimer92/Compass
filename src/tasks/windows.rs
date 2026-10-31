use crate::utils::{cmd, context::ProjectContext, platform, tauri};
use anyhow::Result;
use std::path::PathBuf;

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

/// Compila un release de Tauri para **una** arquitectura de Windows.
///
/// `label` es el sufijo visible (`x86`, `arm64` o vacío para x64). El artefacto
/// se renombra a `<productName> v<version> [label].exe` en su propia carpeta
/// `target/<triple>/release`, así que dos arquitecturas nunca se pisan.
///
/// ⚠️ ARM64 necesita las herramientas ARM64 de Visual Studio (enlazador y
/// librerías); si faltan, el build falla con un mensaje que lo dice.
pub fn build_arch(ctx: &ProjectContext, triple: &str, label: &str) -> Result<PathBuf> {
    tauri::ensure(ctx)?;

    // Best-effort: si el target ya está instalado no hace nada; si falla, el
    // build dará el error real y más claro.
    let _ = cmd::run_program("rustup", &["target", "add", triple], &ctx.script_dir, &[]);

    let display = if label.is_empty() { "x64" } else { label };
    println!("\n[INFO] Compilando escritorio Tauri para {display} ({triple})...\n");
    let command = tauri::command(ctx, &format!("build --target {triple} --bundles nsis"));
    let ok = cmd::run_shell_status(&command, &ctx.script_dir, &[])?;

    let base = tauri_artifact_base(ctx, label);
    let target_dir = ctx
        .script_dir
        .join("src-tauri")
        .join("target")
        .join(triple)
        .join("release");

    if ok {
        let bundle = target_dir.join("bundle").join("nsis");
        if let Some(path) = cmd::newest_file_in(&bundle, platform::desktop_artifact_exts()) {
            return cmd::rename_artifact(&path, &base).ok_or_else(|| {
                anyhow::anyhow!("[ERROR] No se pudo renombrar el instalador {}", path.display())
            });
        }
    }

    // NSIS puede no soportar la arquitectura (x86) o faltar `makensis`; el
    // `.exe` portable ya está compilado y sirve como entrega.
    let release = target_dir.clone();
    if let Some(portable) = cmd::newest_file_in(&release, &["exe"]) {
        println!();
        println!("[WARN] No se genero el instalador NSIS; se entrega el ejecutable portable.");
        let target = release.join(format!("{base}.exe"));
        std::fs::copy(&portable, &target)?;
        return Ok(target);
    }

    anyhow::bail!(
        "[ERROR] Fallo el build de Tauri para {label} ({triple}). \
         En ARM64 suele faltar el enlazador de Visual Studio para ARM64."
    )
}

fn tauri_artifact_base(ctx: &ProjectContext, label: &str) -> String {
    let (name, version) = read_tauri_app_meta(ctx)
        .unwrap_or_else(|| (platform::dir_display_name(&ctx.script_dir), String::new()));
    match (version.is_empty(), label.is_empty()) {
        (true, true) => name,
        (true, false) => format!("{name} {label}"),
        (false, true) => format!("{name} v{version}"),
        (false, false) => format!("{name} v{version} {label}"),
    }
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
