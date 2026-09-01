use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::tasks::signer;
use crate::utils::{cmd, context::{kotlin_android_dir, ProjectContext, ProjectType}, ui};

struct KeystoreInfo {
    path: PathBuf,
    alias: Option<String>,
    store_pass: String,
}

/// Muestra las huellas digitales SHA-1/SHA-256 (debug y release)
pub fn show(ctx: &ProjectContext) -> Result<()> {
    if ctx.project_type == ProjectType::Rust {
        cmd::clear_screen();
        println!("[INFO] No aplicable a proyectos Rust.");
        println!();
        ui::pause();
        return Ok(());
    }

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       HUELLAS DIGITALES ANDROID (SHA-1)          ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let keytool = signer::find_keytool()?;

    // DEBUG
    println!("── DEBUG ──────────────────────────────────────────");
    match debug_keystore() {
        Some(info) => print_fingerprints(&keytool, &info)?,
        None => {
            println!("  [WARN] No se encontró debug.keystore en %USERPROFILE%\\.android\\");
            println!("  Compila una vez en debug o créalo con:");
            println!("    keytool -genkey -v -keystore debug.keystore -storepass android \\");
            println!("      -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000");
        }
    }
    println!();

    // RELEASE
    println!("── RELEASE ────────────────────────────────────────");
    match release_keystore(ctx) {
        Some(Ok(info)) => print_fingerprints(&keytool, &info)?,
        Some(Err(e)) => println!("  [WARN] {}", e),
        None => {
            let props = props_name(ctx.project_type);
            println!("  [INFO] Sin firma release configurada (falta {}).", props);
            println!("  Usa la tarjeta 'Configure' para crearla.");
        }
    }

    println!();
    println!("[INFO] Pega estas huellas en Firebase / Google Cloud Console según lo necesites.");
    println!();
    ui::pause();
    Ok(())
}

fn debug_keystore() -> Option<KeystoreInfo> {
    let home = std::env::var("USERPROFILE").ok()?;
    let path = PathBuf::from(home).join(".android").join("debug.keystore");
    if !path.exists() {
        return None;
    }
    Some(KeystoreInfo {
        path,
        alias: Some("androiddebugkey".to_string()),
        store_pass: "android".to_string(),
    })
}

fn release_keystore(ctx: &ProjectContext) -> Option<Result<KeystoreInfo>> {
    let android_dir = match ctx.project_type {
        ProjectType::Tauri => ctx.script_dir.join("src-tauri\\gen\\android\\app"),
        ProjectType::Flutter => ctx.script_dir.join("android\\app"),
        ProjectType::Kotlin => kotlin_android_dir(ctx),
        ProjectType::Rust => return None,
    };
    let props_path = android_dir.join(props_name(ctx.project_type));
    if !props_path.exists() {
        return None;
    }
    Some(read_props(&android_dir, &props_path))
}

fn props_name(pt: ProjectType) -> &'static str {
    match pt {
        ProjectType::Flutter => "key.properties",
        _ => "keystore.properties",
    }
}

fn read_props(android_dir: &Path, props_path: &Path) -> Result<KeystoreInfo> {
    let content = fs::read_to_string(props_path)
        .with_context(|| format!("No se pudo leer {}", props_path.display()))?;

    let mut store_file = None;
    let mut key_alias = None;
    let mut store_pass = None;

    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"');
            match k.trim() {
                "storeFile" => store_file = Some(v.to_string()),
                "keyAlias" | "alias" => key_alias = Some(v.to_string()),
                "storePassword" | "storepass" => store_pass = Some(v.to_string()),
                _ => {}
            }
        }
    }

    let store_file = store_file
        .filter(|s| !s.is_empty())
        .context("keystore.properties no tiene storeFile")?;
    let store_pass = store_pass.unwrap_or_default();

    let path = PathBuf::from(&store_file);
    let path = if path.is_absolute() {
        path
    } else {
        android_dir.join(&store_file)
    };

    if !path.exists() {
        anyhow::bail!(
            "El keystore '{}' no existe en {}",
            store_file,
            android_dir.display()
        );
    }

    Ok(KeystoreInfo { path, alias: key_alias, store_pass })
}

fn print_fingerprints(keytool: &str, info: &KeystoreInfo) -> Result<()> {
    println!("  Keystore: {}", info.path.display());
    if let Some(alias) = &info.alias {
        println!("  Alias   : {}", alias);
    }

    let out = Command::new(keytool)
        .args(["-list", "-v"])
        .arg("-keystore")
        .arg(&info.path)
        .args(["-storepass", &info.store_pass])
        .output()
        .context("Error al ejecutar keytool")?;

    if !out.status.success() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let msg = combined
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("error desconocido");
        println!("  [ERROR] No se pudo leer el keystore: {}", msg);
        return Ok(());
    }

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let t = line.trim();
        for prefix in ["SHA1:", "SHA256:", "MD5:", "SHA-1:", "SHA-256:"] {
            if let Some(rest) = t.strip_prefix(prefix) {
                let name = prefix.trim_end_matches(':').replace('-', "");
                let label = match name.as_str() {
                    "SHA1" => "SHA-1  ",
                    "SHA256" => "SHA-256",
                    _ => "MD5    ",
                };
                println!("  {} : {}", label, rest.trim());
            }
        }
    }
    Ok(())
}
