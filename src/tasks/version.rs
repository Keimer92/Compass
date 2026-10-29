use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use regex::Regex;

use crate::utils::cmd;
use crate::utils::context::{ProjectContext, ProjectType};
use crate::utils::ui;

const RE_JSON: &str = r#""version"\s*:\s*"([^"]+)""#;
const RE_TOML: &str = r#"(?m)^version\s*=\s*"([^"]+)""#;
/// Lectura de la versión en YAML (`build/config.yml` de Wails v3).
const RE_YAML: &str = r#"(?m)^[ \t]*version:[ \t]*"([^"]+)""#;
/// Escritura en YAML: el grupo 1 es la indentación, que se conserva.
const RE_YAML_REPLACE: &str = r#"(?m)^([ \t]*)version:[ \t]*"[^"]+""#;
const RE_PYPROJECT_VERSION: &str = r#"(?m)^([ \t]*version[ \t]*=[ \t]*")[^"]+(".*)$"#;
const RE_PACKAGE: &str = r"(?m)^\[package\]\s*$";
const RE_NEW_VERSION: &str = r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$";

/// Formato del fichero de configuración, para leer y escribir la versión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toml,
    Yaml,
    Pyproject,
}

struct VersionFile {
    path: PathBuf,
    format: Format,
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

    let configs: Vec<(PathBuf, Format)> = match ctx.project_type {
        ProjectType::Tauri => vec![
            (ctx.script_dir.join("package.json"), Format::Json),
            (
                ctx.script_dir.join("src-tauri").join("tauri.conf.json"),
                Format::Json,
            ),
            (
                ctx.script_dir.join("src-tauri").join("Cargo.toml"),
                Format::Toml,
            ),
        ],
        ProjectType::Rust => vec![(ctx.script_dir.join("Cargo.toml"), Format::Toml)],
        // Wails v3 lleva la versión en `build/config.yml` (bloque `info:`).
        ProjectType::GoWails => vec![(
            ctx.script_dir.join("build").join("config.yml"),
            Format::Yaml,
        )],
        ProjectType::Python => vec![(ctx.script_dir.join("pyproject.toml"), Format::Pyproject)],
        ProjectType::Flutter | ProjectType::Kotlin | ProjectType::Go => {
            println!("[INFO] No aplicable: la version se gestiona en la configuracion propia del proyecto.");
            println!();
            pause();
            return Ok(());
        }
    };

    // Fase de VALIDACION (todo-o-nada): si algo falla, no se escribe nada.
    let mut files: Vec<VersionFile> = Vec::new();
    for (path, format) in configs {
        if !path.exists() {
            bail!("No existe el archivo: {}", path.display());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("No se pudo leer: {}", path.display()))?
            .trim_start_matches('\u{feff}')
            .to_string();

        let current = if format == Format::Pyproject {
            pyproject_version(&text)
        } else {
            let re = Regex::new(match format {
                Format::Json => RE_JSON,
                Format::Toml => RE_TOML,
                Format::Yaml => RE_YAML,
                Format::Pyproject => unreachable!(),
            })?;
            re.captures(&text).map(|cap| cap[1].to_string())
        };
        if let Some(current) = current {
            files.push(VersionFile {
                path,
                format,
                current: Some(current),
                text,
            });
            continue;
        }

        if format == Format::Pyproject {
            bail!(
                "No se encontró una versión estática dentro de `[project]` o `[tool.poetry]` en {}. \
                 Los proyectos con version dinámica deben actualizarla en su sistema de build.",
                path.display()
            );
        }
        // Sin campo version: comprobar que se puede INSERTAR de forma segura.
        if format == Format::Yaml {
            // En YAML insertar a ciegas rompería el bloque `info:`; se exige
            // que la versión ya exista.
            bail!(
                "No se encontro el campo `version:` en: {}\n  \
                 Añádelo bajo `info:` y vuelve a intentarlo.",
                path.display()
            );
        }
        if format == Format::Toml {
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
                bail!(
                    "No se encontro el inicio del objeto JSON: {}",
                    path.display()
                );
            }
        }
        files.push(VersionFile {
            path,
            format,
            current: None,
            text,
        });
    }

    // Mostrar versiones actuales
    println!(
        "  Tipo de proyecto: {}",
        match ctx.project_type {
            ProjectType::Tauri => "TAURI",
            ProjectType::Go => "GO",
            ProjectType::GoWails => "GO + WAILS",
            ProjectType::Python => "PYTHON",
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
            Some(_) => match f.format {
                Format::Json => {
                    let re = Regex::new(RE_JSON)?;
                    re.replace(&f.text, format!("\"version\": \"{}\"", new).as_str())
                        .into_owned()
                }
                Format::Toml => {
                    let re = Regex::new(RE_TOML)?;
                    re.replace(&f.text, format!("version = \"{}\"", new).as_str())
                        .into_owned()
                }
                // La indentación se conserva con `$1`.
                Format::Yaml => {
                    let re = Regex::new(RE_YAML_REPLACE)?;
                    re.replace(&f.text, format!("${{1}}version: \"{}\"", new).as_str())
                        .into_owned()
                }
                Format::Pyproject => replace_pyproject_version(&f.text, &new)?,
            },
            None => {
                added += 1;
                if f.format == Format::Toml {
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

    println!(
        "[OK] Version actualizada a {} en {} archivo(s).",
        new,
        files.len()
    );
    if added > 0 {
        println!(
            "[OK] Se inserto el campo version en {} archivo(s) que no lo tenian.",
            added
        );
    }
    println!();
    pause();
    Ok(())
}

fn toml_section_range(text: &str, name: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if start.is_none() {
            if trimmed == name {
                start = Some(offset + line.len());
            }
        } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return Some((start?, offset));
        }
        offset += line.len();
    }
    start.map(|s| (s, text.len()))
}

fn pyproject_version(text: &str) -> Option<String> {
    let re = Regex::new(RE_TOML).ok()?;
    for section in ["[project]", "[tool.poetry]"] {
        let Some((start, end)) = toml_section_range(text, section) else {
            continue;
        };
        if let Some(cap) = re.captures(&text[start..end]) {
            return Some(cap[1].to_string());
        }
    }
    None
}

fn replace_pyproject_version(text: &str, version: &str) -> Result<String> {
    let re = Regex::new(RE_PYPROJECT_VERSION)?;
    for name in ["[project]", "[tool.poetry]"] {
        let Some((start, end)) = toml_section_range(text, name) else {
            continue;
        };
        let section = &text[start..end];
        if re.is_match(section) {
            let updated = re
                .replace(section, format!("${{1}}{}${{2}}", version).as_str())
                .into_owned();
            return Ok(format!("{}{}{}", &text[..start], updated, &text[end..]));
        }
    }
    bail!("No se encontró una versión estática en [project] ni [tool.poetry]")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El `version:` de `build/config.yml` de Wails v3 va indentado y con un
    /// comentario detrás: la escritura debe conservar ambas cosas.
    #[test]
    fn yaml_version_replacement_keeps_indent_and_comment() {
        let text =
            "info:\n  productName: \"App\"\n  version: \"0.0.1\"  # The application version\n";
        let re = Regex::new(RE_YAML).unwrap();
        let cap = re.captures(text).unwrap();
        assert_eq!(cap.get(1).unwrap().as_str(), "0.0.1");

        let re = Regex::new(RE_YAML_REPLACE).unwrap();
        let out = re.replace(text, "${1}version: \"1.2.3\"").into_owned();
        assert!(out.contains("  version: \"1.2.3\"  # The application version"));
    }

    /// Las líneas comentadas (`# version: ...`) no son la versión del proyecto.
    #[test]
    fn yaml_version_ignores_commented_lines() {
        let text = "info:\n  # version: \"9.9.9\"\n  version: \"0.0.1\"\n";
        let re = Regex::new(RE_YAML).unwrap();
        assert_eq!(re.captures(text).unwrap().get(1).unwrap().as_str(), "0.0.1");
    }

    #[test]
    fn pyproject_version_update_is_limited_to_project_section() {
        let text = "[build-system]\nversion = \"8.0.0\"\n\n[project]\nname = \"demo\"\nversion = \"1.0.0\" # release\n\n[tool.other]\nversion = \"2.0.0\"\n";
        assert_eq!(pyproject_version(text).as_deref(), Some("1.0.0"));
        let updated = replace_pyproject_version(text, "1.1.0").unwrap();
        assert!(updated.contains("version = \"1.1.0\" # release"));
        assert!(updated.contains("[build-system]\nversion = \"8.0.0\""));
        assert!(updated.contains("[tool.other]\nversion = \"2.0.0\""));
    }

    #[test]
    fn poetry_version_updates_its_own_section() {
        let text = "[tool.poetry]\nname = \"demo\"\nversion = \"1.0.0\"\n\n[tool.other]\nversion = \"2.0.0\"\n";
        assert_eq!(pyproject_version(text).as_deref(), Some("1.0.0"));
        let updated = replace_pyproject_version(text, "1.1.0").unwrap();
        assert!(updated.contains("[tool.poetry]\nname = \"demo\"\nversion = \"1.1.0\""));
        assert!(updated.contains("[tool.other]\nversion = \"2.0.0\""));
    }
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    use std::io::Write;
    std::io::stdout().flush().ok();
    let mut _input = String::new();
    std::io::stdin().read_line(&mut _input).ok();
}
