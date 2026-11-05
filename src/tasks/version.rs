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
const RE_GO_CONST: &str = r#"(?m)^[ \t]*const[ \t]+version[ \t]*=[ \t]*"([^"]+)""#;
const RE_GO_CONST_REPLACE: &str = r#"(?m)^([ \t]*const[ \t]+version[ \t]*=[ \t]*")[^"]+(")"#;
const RE_NSIS: &str = r#"(?m)^!define[ \t]+INFO_PRODUCTVERSION[ \t]+"([^"]+)""#;
const RE_NSIS_REPLACE: &str = r#"(?m)^(!define[ \t]+INFO_PRODUCTVERSION[ \t]+")[^"]+(")"#;
/// `CFBundleShortVersionString` y `CFBundleVersion` de los plists de
/// iOS/macOS: se sustituyen ambos dentro del mismo archivo.
const RE_PLIST: &str = r"(?ms)(<key>CFBundle(?:ShortVersionString|Version)</key>\s*<string>)[^<]*?(</string>)";
const RE_PLIST_FIRST: &str = r"(?ms)<key>CFBundleShortVersionString</key>\s*<string>([^<]*)</string>";
/// `build/windows/info.json` de Wails: `file_version` y `product_version`
/// llevan CUATRO partes (`0.1.1.0`); `ProductVersion` (info.0409) lleva tres.
const RE_IJ_FILE: &str = r#""file_version"\s*:\s*"([^"]+)""#;
const RE_IJ_PRODUCT: &str = r#""product_version"\s*:\s*"([^"]+)""#;
const RE_IJ_PRODUCT_CAP: &str = r#""ProductVersion"\s*:\s*"([^"]+)""#;
/// Gradle: `versionName` lleva la versión; `versionCode` es un entero que
/// SIEMPRE debe incrementarse para que Android acepte la actualización.
const RE_GRADLE_NAME: &str = r#"(?m)^[ \t]*versionName[ \t]+"([^"]+)""#;
const RE_GRADLE_CODE: &str = r#"(?m)^[ \t]*versionCode[ \t]+(\d+)"#;
const RE_GRADLE_NAME_REPLACE: &str = r#"(?m)^([ \t]*versionName[ \t]+")[^"]+(")"#;
const RE_GRADLE_CODE_REPLACE: &str = r#"(?m)^([ \t]*versionCode[ \t]+)(\d+)"#;
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
    /// `const version = "x.y.z"` en Go (p. ej. `main.go` de school-tracker).
    GoConst,
    /// `!define INFO_PRODUCTVERSION "x.y.z"` de NSIS.
    Nsis,
    /// `build/windows/info.json` de Wails (tres versiones en el mismo JSON).
    InfoJson,
    /// `versionName` + `versionCode` de Gradle.
    Gradle,
    /// Plists de iOS/macOS (`CFBundleShortVersionString` / `CFBundleVersion`).
    Plist,
}

/// Si un archivo esperado es obligatorio o sólo "si existe".
/// En un proyecto Go+Wails genérico no todos tienen NSIS, Android o iOS:
/// los que faltan se omiten, pero si EXISTEN y no encajan, se aborta todo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Required,
    Optional,
}

struct SourceFile {
    path: PathBuf,
    format: Format,
    source: Source,
}

struct VersionFile {
    path: PathBuf,
    format: Format,
    current: Option<String>,
    /// Gradle: el `versionCode` actual, para escribirlo incrementado.
    current_code: Option<u64>,
    text: String,
}

/// Actualiza la version del proyecto (port de `versionar.bat`).
/// Tauri: package.json + src-tauri/tauri.conf.json + src-tauri/Cargo.toml.
/// Rust: Cargo.toml en la raiz.
/// Go + Wails: `build/config.yml` +, si existen, `main.go` (`const version`),
///   `frontend/package.json`, NSIS (`project.nsi`), `build/windows/info.json`,
///   Android (`build.gradle` de `app/` y `custom/app/`, incrementando
///   `versionCode`) e iOS/macOS (`Info*.plist`).
/// Todo-o-nada: lee y valida todos los archivos antes de escribir ninguno.
/// Los opcionales que no existen se omiten; los que existen y no encajan
/// abortan el conjunto completo.
pub fn bump(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              VERSIONADOR DE PROYECTO              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let configs: Vec<SourceFile> = match ctx.project_type {
        ProjectType::Tauri => [
            ("package.json", Format::Json),
            ("src-tauri/tauri.conf.json", Format::Json),
            ("src-tauri/Cargo.toml", Format::Toml),
        ]
        .into_iter()
        .map(|(p, f)| SourceFile {
            path: ctx.script_dir.join(p),
            format: f,
            source: Source::Required,
        })
        .collect(),
        ProjectType::Rust => vec![SourceFile {
            path: ctx.script_dir.join("Cargo.toml"),
            format: Format::Toml,
            source: Source::Required,
        }],
        // Wails v3 lleva la versión generada en `build/config.yml` (bloque
        // `info:`), pero la que la app muestra y el updater compara vive en
        // `main.go`; y las metadatas de Windows/Android/iOS/macOS suelen ir
        // atadas a la misma versión. Se actualizan todas; los opcionales que
        // no existan (p. ej. NSIS en un proyecto sin Windows) se omiten.
        ProjectType::GoWails => [
            ("build/config.yml", Format::Yaml, Source::Required),
            ("main.go", Format::GoConst, Source::Optional),
            ("frontend/package.json", Format::Json, Source::Optional),
            (
                "build/windows/nsis/project.nsi",
                Format::Nsis,
                Source::Optional,
            ),
            (
                "build/windows/info.json",
                Format::InfoJson,
                Source::Optional,
            ),
            (
                "build/android/app/build.gradle",
                Format::Gradle,
                Source::Optional,
            ),
            (
                "build/android/custom/app/build.gradle",
                Format::Gradle,
                Source::Optional,
            ),
            ("build/ios/Info.plist", Format::Plist, Source::Optional),
            ("build/ios/Info.dev.plist", Format::Plist, Source::Optional),
            ("build/darwin/Info.plist", Format::Plist, Source::Optional),
            ("build/darwin/Info.dev.plist", Format::Plist, Source::Optional),
        ]
        .into_iter()
        .map(|(p, f, s)| SourceFile {
            path: ctx.script_dir.join(p),
            format: f,
            source: s,
        })
        .collect(),
        ProjectType::Python => vec![SourceFile {
            path: ctx.script_dir.join("pyproject.toml"),
            format: Format::Pyproject,
            source: Source::Required,
        }],
        ProjectType::Flutter | ProjectType::Kotlin | ProjectType::Go => {
            println!("[INFO] No aplicable: la version se gestiona en la configuracion propia del proyecto.");
            println!();
            pause();
            return Ok(());
        }
    };

    // Fase de VALIDACION (todo-o-nada): si algo falla, no se escribe nada.
    let mut files: Vec<VersionFile> = Vec::new();
    let mut omitted: Vec<PathBuf> = Vec::new();
    for SourceFile { path, format, source } in configs {
        if !path.exists() {
            match source {
                Source::Required => bail!("No existe el archivo: {}", path.display()),
                Source::Optional => {
                    omitted.push(path);
                    continue;
                }
            }
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("No se pudo leer: {}", path.display()))?
            .trim_start_matches('\u{feff}')
            .to_string();

        let current_code = if format == Format::Gradle {
            gradle_code(&text)?
        } else {
            None
        };
        let current = detect_current(format, &text)?;
        if let Some(current) = current {
            if format == Format::Gradle && current_code.is_none() {
                bail!(
                    "No se encontro `versionCode` en: {}\n  \
                     Anadelo dentro del bloque defaultConfig.",
                    path.display()
                );
            }
            files.push(VersionFile {
                path,
                format,
                current: Some(current),
                current_code,
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
        // Los formatos Wails/NSIS/Gradle/Plist no admiten insertar a ciegas:
        // que el marcador exista ya es parte del contrato del archivo.
        if format != Format::Json && format != Format::Toml {
            bail!(
                "No se encontro el marcador de version en: {}\n  {}",
                path.display(),
                missing_hint(format)
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
            current_code: None,
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
    for path in &omitted {
        let rel = path.strip_prefix(&ctx.script_dir).unwrap_or(path);
        println!("  (no existe, se omite)   {}", rel.display());
    }
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
    let mut gradle_notes: Vec<(PathBuf, u64)> = Vec::new();
    for f in &mut files {
        let new_text = match &f.current {
            Some(_) => {
                if f.format == Format::Gradle {
                    let old_code = f
                        .current_code
                        .with_context(|| "versionCode ausente tras la validacion")?;
                    gradle_notes.push((f.path.clone(), old_code));
                }
                apply_format(f, &new)?
            }
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
    for (path, old_code) in &gradle_notes {
        let rel = path.strip_prefix(&ctx.script_dir).unwrap_or(path);
        println!(
            "[OK] versionCode {} -> {} en {}",
            old_code,
            old_code + 1,
            rel.display()
        );
    }
    println!();
    pause();
    Ok(())
}

/// Primera captura de un patrón, en el texto entero.
fn cap1(re: &str, text: &str) -> Result<Option<String>> {
    Ok(Regex::new(re)?.captures(text).map(|c| c[1].to_string()))
}

/// `versionCode` numérico, para escribirlo incrementado.
fn gradle_code(text: &str) -> Result<Option<u64>> {
    Ok(Regex::new(RE_GRADLE_CODE)?
        .captures(text)
        .and_then(|c| c[1].parse::<u64>().ok()))
}

/// Marcador ausente: qué se esperaba encontrar en ese archivo.
fn missing_hint(format: Format) -> &'static str {
    match format {
        Format::Yaml => "se esperaba `version: \"x.y.z\"` bajo `info:` (ver build/config.yml)",
        Format::Json => "se esperaba una propiedad `version` en el objeto raíz",
        Format::Toml => "se esperaba `version = \"x.y.z\"`",
        Format::GoConst => "se esperaba `const version = \"x.y.z\"` (p. ej. main.go)",
        Format::Nsis => "se esperaba `!define INFO_PRODUCTVERSION \"x.y.z\"` (project.nsi)",
        Format::InfoJson => "se esperaban `file_version`, `product_version` y `ProductVersion` (info.json)",
        Format::Gradle => "se esperaba `versionName \"x.y.z\"` (build.gradle)",
        Format::Plist => "se esperaban `CFBundleShortVersionString` y `CFBundleVersion` (Info.plist)",
        Format::Pyproject => "se esperaba `version` bajo [project] o [tool.poetry]",
    }
}

/// Lee la versión del archivo según su formato.
fn detect_current(format: Format, text: &str) -> Result<Option<String>> {
    match format {
        Format::Json => cap1(RE_JSON, text),
        Format::Toml => cap1(RE_TOML, text),
        Format::Yaml => cap1(RE_YAML, text),
        Format::Pyproject => Ok(pyproject_version(text)),
        Format::GoConst => cap1(RE_GO_CONST, text),
        Format::Nsis => cap1(RE_NSIS, text),
        Format::InfoJson => {
            let file = cap1(RE_IJ_FILE, text)?;
            let product = cap1(RE_IJ_PRODUCT, text)?;
            let product_cap = cap1(RE_IJ_PRODUCT_CAP, text)?;
            // Los tres campos tienen que encajar; si falta cualquiera, se baila.
            match (file, product, product_cap) {
                (Some(f), Some(_), Some(_)) => Ok(Some(f)),
                _ => Ok(None),
            }
        }
        Format::Gradle => cap1(RE_GRADLE_NAME, text),
        Format::Plist => cap1(RE_PLIST_FIRST, text),
    }
}

/// Aplica la nueva versión a un archivo ya validado.
fn apply_format(f: &VersionFile, new: &str) -> Result<String> {
    match f.format {
        Format::Json => Ok(Regex::new(RE_JSON)?
            .replace(&f.text, format!("\"version\": \"{new}\"").as_str())
            .into_owned()),
        Format::Toml => Ok(Regex::new(RE_TOML)?
            .replace(&f.text, format!("version = \"{new}\"").as_str())
            .into_owned()),
        // La indentación se conserva con `$1`.
        Format::Yaml => Ok(Regex::new(RE_YAML_REPLACE)?
            .replace(&f.text, format!("${{1}}version: \"{new}\"").as_str())
            .into_owned()),
        Format::Pyproject => replace_pyproject_version(&f.text, new),
        Format::GoConst => Ok(Regex::new(RE_GO_CONST_REPLACE)?
            .replace(&f.text, format!("${{1}}{new}${{2}}").as_str())
            .into_owned()),
        Format::Nsis => Ok(Regex::new(RE_NSIS_REPLACE)?
            .replace(&f.text, format!("${{1}}{new}${{2}}").as_str())
            .into_owned()),
        // `file_version` y `product_version`: cuatro partes; `ProductVersion`: tres.
        Format::InfoJson => {
            let first = Regex::new(RE_IJ_FILE)?
                .replace(&f.text, format!("\"file_version\": \"{new}.0\"").as_str())
                .into_owned();
            let second = Regex::new(RE_IJ_PRODUCT)?
                .replace(&first, format!("\"product_version\": \"{new}.0\"").as_str())
                .into_owned();
            Ok(Regex::new(RE_IJ_PRODUCT_CAP)?
                .replace(&second, format!("\"ProductVersion\": \"{new}\"").as_str())
                .into_owned())
        }
        Format::Gradle => {
            let renamed = Regex::new(RE_GRADLE_NAME_REPLACE)?
                .replace(&f.text, format!("${{1}}{new}${{2}}").as_str())
                .into_owned();
            let code = f
                .current_code
                .with_context(|| "versionCode ausente tras la validacion")?;
            Ok(Regex::new(RE_GRADLE_CODE_REPLACE)?
                .replace(&renamed, format!("${{1}}{}", code + 1).as_str())
                .into_owned())
        }
        // Ambas claves (CFBundleShortVersionString y CFBundleVersion) se reescriben.
        Format::Plist => Ok(Regex::new(RE_PLIST)?
            .replace_all(&f.text, format!("${{1}}{new}${{2}}").as_str())
            .into_owned()),
    }
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
    fn go_const_replacement_ignores_commented_lines() {
        let text = "// const version = \"9.9.9\"\nconst version = \"0.1.1\"\n";
        let out = Regex::new(RE_GO_CONST_REPLACE)
            .unwrap()
            .replace(text, "${1}0.2.0${2}")
            .into_owned();
        assert!(out.contains("const version = \"0.2.0\""));
        assert!(out.contains("// const version = \"9.9.9\""));
    }

    #[test]
    fn nsis_replacement_ignores_commented_defines() {
        let text =
            "## !define INFO_PRODUCTVERSION \"1.0.0\"  # default\n!define INFO_PRODUCTVERSION \"0.1.1\"\n";
        let out = Regex::new(RE_NSIS_REPLACE)
            .unwrap()
            .replace(text, "${1}0.2.0${2}")
            .into_owned();
        assert!(out.contains("## !define INFO_PRODUCTVERSION \"1.0.0\""));
        assert!(out.contains("!define INFO_PRODUCTVERSION \"0.2.0\""));
    }

    #[test]
    fn gradle_replacements_bump_name_and_code() {
        let text = "        versionCode 2\n        versionName \"0.1.1\"\n";
        let renamed = Regex::new(RE_GRADLE_NAME_REPLACE)
            .unwrap()
            .replace(text, "${1}0.2.0${2}")
            .into_owned();
        assert_eq!(detect_current(Format::Gradle, text).unwrap().as_deref(), Some("0.1.1"));
        assert_eq!(gradle_code(text).unwrap(), Some(2));
        let out = Regex::new(RE_GRADLE_CODE_REPLACE)
            .unwrap()
            .replace(&renamed, "${1}3")
            .into_owned();
        assert!(out.contains("versionName \"0.2.0\""));
        assert!(out.contains("versionCode 3"));
    }

    #[test]
    fn plist_rewrites_short_and_bundle_version() {
        let text = "<key>CFBundleShortVersionString</key>\n    <string>0.1.0</string>\n<key>CFBundleVersion</key>\n    <string>0.1.0</string>\n";
        let out = Regex::new(RE_PLIST)
            .unwrap()
            .replace_all(text, "${1}0.2.0${2}")
            .into_owned();
        assert_eq!(out.matches("<string>0.2.0</string>").count(), 2);
    }

    #[test]
    fn infojson_handles_four_part_and_three_part_fields() {
        let text = concat!(
            "{\n\t\"fixed\": {\n\t\t\"file_version\": \"0.1.1.0\",\n\t\t\"product_version\": \"0.1.1.0\"\n\t},\n",
            "\t\"info\": {\n\t\t\"0409\": {\n\t\t\t\"ProductVersion\": \"0.1.1\"\n\t\t}\n\t}\n}"
        );
        let f = VersionFile {
            path: PathBuf::from("info.json"),
            format: Format::InfoJson,
            current: detect_current(Format::InfoJson, text).unwrap(),
            current_code: None,
            text: text.to_string(),
        };
        assert_eq!(f.current.as_deref(), Some("0.1.1.0"));
        let out = apply_format(&f, "0.2.0").unwrap();
        assert!(out.contains("\"file_version\": \"0.2.0.0\""));
        assert!(out.contains("\"product_version\": \"0.2.0.0\""));
        assert!(out.contains("\"ProductVersion\": \"0.2.0\""));
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
