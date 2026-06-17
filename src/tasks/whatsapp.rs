use std::io::{self, Write};
use anyhow::Result;
use crate::utils::{cmd, context::ProjectContext};

pub fn build(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║         COMPILAR MOTOR DE WHATSAPP               ║");
    println!("╚══════════════════════════════════════════════════╝");

    let engine_js = ctx.script_dir.join("src-tauri\\whatsapp-engine\\engine.js");
    let bundle_mjs = ctx.script_dir.join("src-tauri\\whatsapp-engine\\bundle.mjs");

    if !engine_js.exists() {
        eprintln!();
        eprintln!("[ERROR] No se encontró el motor de WhatsApp en este proyecto.");
        eprintln!("  Ruta esperada: src-tauri\\whatsapp-engine\\engine.js");
        eprintln!();
        eprintln!("  Este proyecto no tiene un motor de WhatsApp integrado.");
        pause();
        return Ok(());
    }

    if !bundle_mjs.exists() {
        eprintln!();
        eprintln!("[ERROR] No se encontró el script de compilación del motor.");
        eprintln!("  Ruta esperada: src-tauri\\whatsapp-engine\\bundle.mjs");
        pause();
        return Ok(());
    }

    // Install engine dependencies
    println!();
    println!("[INFO] Instalando dependencias del motor...");
    let engine_dir = ctx.script_dir.join("src-tauri\\whatsapp-engine");
    if engine_dir.exists() {
        cmd::run_in_dir_status("npm", &["install"], &engine_dir)?;
    }

    // Build with ncc
    println!("[INFO] Compilando motor de WhatsApp con ncc...\n");

    let output = cmd::run_in_dir("node", &[bundle_mjs.to_str().unwrap()], &ctx.script_dir)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[ERROR] Falló la compilación del motor:\n{}", stderr);
        pause();
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("\n{}", stdout);
    println!("[OK] Motor de WhatsApp compilado exitosamente!");

    let bundle_dir = ctx.script_dir.join("src-tauri\\resources\\whatsapp-engine-bundle");
    if bundle_dir.exists() {
        let total_size: u64 = std::fs::read_dir(&bundle_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
        println!("  Bundle generado en: {}", bundle_dir.display());
        println!("  Tamaño total del bundle: {:.2} MB", total_size as f64 / 1_048_576.0);
    }

    println!();
    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
