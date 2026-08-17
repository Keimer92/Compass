use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use regex::Regex;

use crate::utils::cmd;
use crate::utils::context::{ProjectContext, ProjectType};
use crate::utils::ui;

const RE_JSON: &str = r#""version"\s*:\s*"([^"]+)""#;
const RE_TOML: &str = r"(?m)^version\s*=\s*"([^"]+)""#;
const RE_PACKAGE: &str = r"(?m)^\[package\]\s*$";
const RE_NEW_VERSION: &str = r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$";

struct VersionFile {
    path: PathBuf,
    is_json: bool,
    current: Option<String>,
    text: String,
}

/// Actualiza la version del proyecto (port de `versionar.bat`).
/// Tauri: package.json + src-tauri/tauri.conf.json + src-tauri/Cargo.toml.
/// Rust: Cargo.toml en la raiz.
/// Todo-o-nada: lee y valida todos los archivos antes de escribir ninguno.
pub fn bump(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              VERSIONADOR DE PROYECTO              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let configs: Vec<(PathBuf, bool)> = match ctx.project_type {
        ProjectType::Tauri => vec![
            (ctx.script_dir.join("package.json"), true),
            (ctx.script_dir.join("src-tauri").join("tauri.conf.json"), true),
            (ctx.script_dir.join("src-tauri").join("Cargo.toml"), false),
        ],
        ProjectType::Rust => vec![(ctx.script_dir.join("Cargo.toml"), false)],
        ProjectType::Flutter | ProjectType::Kotlin => {
            println!("[INFO] No aplicable: en Flutter/Kotlin la version se gestiona en pubspec.yaml / gradle.");
            println!();
            pause();
            return Ok(());
        }
    };

    // Fase de VALIDACION (todo-o-nada): si algo falla, no se escribe nada.
    let mut files: Vec<VersionFile> = Vec::new();
    for (path, is_json) in configs {
        if !path.exists() {
            bail!("No existe el archivo: {}", path.display());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("No se pudo leer: {}", path.display()))?;

        let re = Regex::new(if is_json { RE_JSON } else { RE_TOML })?;
        if let Some(cap) = re.captures(&text) {
            files.push(VersionFile {
                path,
                is_json,
                current: Some(cap[1].to_string()),
                text,
            });
            continue;
        }

        // Sin campo version: comprobar que se puede INSERTAR de forma segura.
        if !is_json {
            if !Regex::new(RE_PACKAGE)?.is_match(&text) {
                bail!(
                    "Cargo.toml sin version y sin seccion [package]:\n  no se puede insertar de forma segura: {}",
                    path.display()
                );
            }
        } else {
            let validate = text.trim_start_matches('\u{feff}');
            let value: serde_json::Value = serde_json::from_str(validate).with_context(|| {
                format!(
                    "JSON invalido; no se puede insertar version de forma segura: {}",
                    path.display()
                )
            })?;
            if !value.is_object() {
                bail!(
                    "JSON no es un objeto; no se puede insertar version de forma segura: {}",
                    path.display()
                );
            }
            if text.find('{').is_none() {
                bail!("No se encontro el inicio del objeto JSON: {}", path.display());
            }
        }
        files.push(VersionFile {
            path,
            is_json,
            current: None,
            text,
        });
    }

    // Mostrar versiones actuales
    println!(
        "  Tipo de proyecto: {}",
        match ctx.project_type {
            ProjectType::Tauri => "TAURI",
            _ => "RUST",
        }
    );
    println!();
    for f in &files {
        let rel = f.path.strip_prefix(&ctx.script_dir).unwrap_or(&f.path);
        let cur = match &f.current {
            Some(v) => v.clone(),
            None => "(sin version - se agregara)".to_string(),
        };
        println!("  {:<34} : {}", rel.display(), cur);
    }
    println!();

    // Pedir la nueva version
    let new = ui::ask_text("Nueva version (X.Y.Z, opcional sufijo -pre/+build)", None)?;
    let new = new.trim().to_string();
    if new.is_empty() {
        bail!("Version vacia.");
    }
    if !Regex::new(RE_NEW_VERSION)?.is_match(&new) {
        bail!("Formato invalido. Usa X.Y.Z (ej: 1.2.3) o con sufijo (ej: 2.0.0-beta.1)");
    }

    // Escribir: reemplaza donde existe, INSERTA donde falta.
    let mut added = 0usize;
    for f in &mut files {
        let new_text = match &f.current {
            Some(_) => {
                let re = Regex::new(if f.is_json { RE_JSON } else { RE_TOML })?;
                let replacement = if f.is_json {
                    format!("\"version\": \"{}\"", new)
                } else {
                    format!("version = \"{}\"", new)
                };
                re.replace(&f.text, replacement.as_str()).into_owned()
            }
            None => {
                added += 1;
                if !f.is_json {
                    let insert = format!("[package]\nversion = \"{}\"", new);
                    let re = Regex::new(RE_PACKAGE)?;
                    re.replace(&f.text, insert.as_str()).into_owned()
                } else {
                    let idx = f.text.find('{').expect("ya validado antes");
                    let prop = format!("\"version\": \"{}\"", new);
                    if f.text[idx + 1..].trim_start().starts_with('}') {
                        format!("{}{}{}", &f.text[..idx + 1], prop, &f.text[idx + 1..])
                    } else {
                        format!("{}{}, {}", &f.text[..idx + 1], prop, &f.text[idx + 1..])
                    }
                }
            }
        };
        std::fs::write(&f.path, new_text)
            .with_context(|| format!("No se pudo escribir: {}", f.path.display()))?;
    }

    println!("[OK] Version actualizada a {} en {} archivo(s).", new, files.len());
    if added > 0 {
        println!("[OK] Se inserto el campo version en {} archivo(s) que no lo tenian.", added);
    }
    println!();
    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    use std::io::Write;
    std::io::stdout().flush().ok();
    let mut _input = String::new();
    std::io::stdin().read_line(&mut _input).ok();
}