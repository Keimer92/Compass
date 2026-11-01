//! Configuración del proveedor **InferX** en OpenCode y recordatorio de la API.
//!
//! OpenCode ya trae el endpoint de InferX en la configuración del usuario, pero
//! una instalación limpia no lo tiene. Compass hace dos cosas al instalar
//! OpenCode:
//!
//! 1. Añade (o completa) el proveedor `inferx` en `opencode.json(c)`, pidiendo
//!    la API key una sola vez. El usuario sólo tiene que pegarla.
//! 2. Instala un plugin de la TUI de OpenCode que muestra un aviso en el pie
//!    mientras falte la clave, con un botón para descartarlo cuando ya esté.
//!
//! El recordatorio también se pinta en la cabecera de Compass; ambos comparten
//! el mismo estado (`inferx_configured`).

use crate::utils::{cmd, platform, ui};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const PROVIDER: &str = "inferx";
const MODEL: &str = "deepseek-v4.1-flash-1017-basic";
const MODEL_NAME: &str = "DeepSeek V4.1 Flash (InferX)";
const BASE_URL: &str = "https://model.inferx.net/endpoints/v1";
const PACKAGE: &str = "@opencode/ai/providers/openai-compatible";
const API_ENV: &str = "INFERX_API_KEY";
const PLACEHOLDER: &str = "PEGA_TU_API_AQUI";

/// Plugin TUI que Compass copia al directorio de descubrimiento de OpenCode.
const PLUGIN_SOURCE: &str = include_str!("../../assets/opencode/tui.tsx");
const PLUGIN_DIR: &str = "inferx-reminder";

/// Marca local de "el usuario ya vio y descartó el recordatorio".
const REMINDER_FILE: &str = "opencode-inferx-reminder";

/// Directorio global de configuración de OpenCode.
///
/// En Windows es `%USERPROFILE%\.config\opencode`, salvo que `XDG_CONFIG_HOME`
/// lo redirija (OpenCode respeta esa variable también aquí).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("opencode"));
    }
    platform::home_dir().map(|home| home.join(".config").join("opencode"))
}

/// Archivo de configuración que se modificará.
///
/// OpenCode acepta `.json` y `.jsonc`; se respeta el que ya exista y, si no hay
/// ninguno, se crea `opencode.jsonc` (el formato recomendado por la documentación).
fn config_file() -> Option<PathBuf> {
    let dir = config_dir()?;
    let jsonc = dir.join("opencode.jsonc");
    let json = dir.join("opencode.json");

    if jsonc.exists() {
        Some(jsonc)
    } else if json.exists() {
        Some(json)
    } else {
        Some(jsonc)
    }
}

/// ¿Está OpenCode instalado?
///
/// Se comprueba el PATH y, además, la ruta que deja npm global en Windows
/// (`%APPDATA%\npm\opencode.cmd`): el proceso que acaba de instalar OpenCode
/// puede no tener el PATH refrescado todavía.
pub fn opencode_available() -> bool {
    if command_exists("opencode") {
        return true;
    }

    if let Some(appdata) = std::env::var_os("APPDATA") {
        let npm = Path::new(&appdata).join("npm");
        if npm.join("opencode.cmd").exists() || npm.join("opencode.ps1").exists() {
            return true;
        }
    }

    false
}

fn command_exists(name: &str) -> bool {
    let ps = format!("$null -ne (Get-Command -Name '{name}' -ErrorAction SilentlyContinue)");
    cmd::run_ps(&ps)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True")
        .unwrap_or(false)
}

/// Flujo interactivo que se ejecuta al instalar OpenCode.
///
/// Es idempotente: si el proveedor ya tiene una clave utilizable no vuelve a
/// preguntar, pero sí reinstala el plugin de recordatorio (por si falta o está
/// desactualizado).
pub fn configure(interactive: bool) -> Result<()> {
    if !opencode_available() {
        println!("[SKIP] OpenCode no está instalado: no se configura InferX.");
        return Ok(());
    }

    let Some(path) = config_file() else {
        println!("[WARN] No se pudo determinar la carpeta de configuración de OpenCode.");
        return Ok(());
    };

    let mut value = if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("No se pudo leer {}", path.display()))?;
        match parse_jsonc(&text) {
            Ok(value) => value,
            Err(err) => {
                println!("[WARN] No se pudo interpretar {}: {err}", path.display());
                println!("[WARN] No se modificó la configuración de OpenCode.");
                return Ok(());
            }
        }
    } else {
        json!({ "$schema": "https://opencode.ai/config.json" })
    };

    let ya_configurado = inferx_configured_in(&value) || env_set(API_ENV);

    let mut api_key = None;
    if !ya_configurado && interactive {
        println!();
        println!("[?] InferX está disponible en OpenCode, pero falta su API key.");
        println!("    Endpoint: {BASE_URL}");
        println!("    (Enter para dejarla pendiente; también vale {API_ENV})");
        let key = ui::ask_password("    API key de InferX")?;
        if !key.trim().is_empty() {
            api_key = Some(key);
        }
    }

    merge_inferx(&mut value, api_key.as_deref());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("No se pudo crear {}", parent.display()))?;
    }

    if path.exists() {
        backup(&path);
    }

    let rendered =
        serde_json::to_string_pretty(&value).context("No se pudo serializar la config")?;
    std::fs::write(&path, format!("{rendered}\n"))
        .with_context(|| format!("No se pudo escribir {}", path.display()))?;

    println!();
    println!("[OK] Proveedor InferX configurado en {}", path.display());
    if let Some(key) = &api_key {
        println!("[OK] API key guardada ({}…).", short_key(key));
    } else if ya_configurado {
        println!("[SKIP] La API key de InferX ya estaba configurada.");
    } else {
        println!("[INFO] Sin API key todavía. Se puede añadir después:");
        println!("       - variable de entorno {API_ENV}, o");
        println!("       - `compass --task opencode-configure` para volver a pedirla.");
    }

    install_plugin()?;

    // Si se acaba de configurar la clave, el recordatorio ya no aplica: se
    // limpia la marca para que no quede oculta si algún día se borra la clave.
    if ya_configurado || api_key.is_some() || inferx_configured_in(&value) {
        clear_reminder_dismissal();
    }

    Ok(())
}

/// Instala/actualiza el plugin TUI que muestra el recordatorio en OpenCode.
fn install_plugin() -> Result<()> {
    let Some(dir) = config_dir() else {
        return Ok(());
    };
    let plugin_dir = dir.join("plugins").join(PLUGIN_DIR);

    std::fs::create_dir_all(&plugin_dir)
        .with_context(|| format!("No se pudo crear {}", plugin_dir.display()))?;

    let file = plugin_dir.join("tui.tsx");
    std::fs::write(&file, PLUGIN_SOURCE)
        .with_context(|| format!("No se pudo escribir {}", file.display()))?;

    println!("[OK] Recordatorio de InferX instalado en la TUI de OpenCode.");
    println!("     {}", file.display());
    Ok(())
}

fn backup(path: &Path) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let backup = path.with_file_name(format!("{name}.compass.bak"));
    let _ = std::fs::copy(path, backup);
}

fn short_key(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len().max(1));
    }
    chars[..6].iter().collect()
}

/// ¿Está InferX listo para usarse?
///
/// Vale una clave literal en `settings.apiKey`, una referencia `{env:…}` cuya
/// variable exista, o cualquiera de las variables declaradas en `env`.
pub fn inferx_configured_in(value: &Value) -> bool {
    let Some(provider) = value.get("providers").and_then(|p| p.get(PROVIDER)) else {
        return false;
    };

    if let Some(key) = provider
        .get("settings")
        .and_then(|s| s.get("apiKey"))
        .and_then(|k| k.as_str())
    {
        let key = key.trim();
        if !key.is_empty() && key != PLACEHOLDER {
            if let Some(var) = env_reference(key) {
                if env_set(&var) {
                    return true;
                }
            } else {
                return true;
            }
        }
    }

    provider
        .get("env")
        .and_then(|e| e.as_array())
        .map(|envs| envs.iter().filter_map(|v| v.as_str()).any(env_set))
        .unwrap_or(false)
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Extrae `NAME` de una referencia `{env:NAME}`.
fn env_reference(value: &str) -> Option<String> {
    value
        .strip_prefix("{env:")
        .and_then(|rest| rest.strip_suffix('}'))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Inserta o completa el proveedor InferX sin pisar otras claves del usuario.
fn merge_inferx(value: &mut Value, api_key: Option<&str>) {
    {
        let Some(root) = value.as_object_mut() else {
            return;
        };

        let providers = root.entry("providers").or_insert_with(|| json!({}));
        let Some(providers) = providers.as_object_mut() else {
            return;
        };

        let provider = providers.entry(PROVIDER).or_insert_with(|| json!({}));
        let Some(provider) = provider.as_object_mut() else {
            return;
        };

        provider.insert("name".into(), json!("InferX"));
        provider.insert("env".into(), json!([API_ENV]));
        provider.insert("package".into(), json!(PACKAGE));

        let settings = provider.entry("settings").or_insert_with(|| json!({}));
        if let Some(settings) = settings.as_object_mut() {
            settings.insert("baseURL".into(), json!(BASE_URL));
            if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
                settings.insert("apiKey".into(), json!(key));
            }
        }

        let models = provider.entry("models").or_insert_with(|| json!({}));
        if let Some(models) = models.as_object_mut() {
            models
                .entry(MODEL)
                .or_insert_with(|| json!({ "name": MODEL_NAME }));
        }
    }

    // Si el usuario no tiene un modelo por defecto, InferX queda listo para
    // usarse sin pasos extra. Si ya eligió otro, se respeta.
    if let Some(root) = value.as_object_mut() {
        if !root.contains_key("model") {
            root.insert("model".into(), json!(format!("{PROVIDER}/{MODEL}")));
        }
    }
}

/// Interpreta JSON o JSONC (comentarios y comas finales).
fn parse_jsonc(input: &str) -> Result<Value> {
    serde_json::from_str(&strip_jsonc(input)).context("JSON inválido")
}

/// Quita comentarios `//` y `/* */` y comas finales, respetando las cadenas.
fn strip_jsonc(input: &str) -> String {
    let mut without_comments = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escape = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if in_string {
            without_comments.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                without_comments.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    while let Some(&next) = chars.peek() {
                        if next == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some('*') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => without_comments.push(c),
            },
            _ => without_comments.push(c),
        }
    }

    let clean: Vec<char> = without_comments.chars().collect();
    let mut result = String::with_capacity(clean.len());
    let mut in_string = false;
    let mut escape = false;

    for (i, &c) in clean.iter().enumerate() {
        if in_string {
            result.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            result.push(c);
            continue;
        }

        if c == ',' {
            let mut j = i + 1;
            while j < clean.len() && clean[j].is_whitespace() {
                j += 1;
            }
            if j < clean.len() && (clean[j] == '}' || clean[j] == ']') {
                continue;
            }
        }

        result.push(c);
    }

    result
}

// ── Recordatorio en la TUI de Compass ────────────────────────────────

fn reminder_path() -> Option<PathBuf> {
    platform::data_dir().map(|dir| dir.join("compass").join(REMINDER_FILE))
}

/// ¿Hay que enseñar el aviso de "falta la API de InferX"?
pub fn reminder_needed() -> bool {
    if reminder_dismissed() || !opencode_available() {
        return false;
    }
    key_is_missing()
}

/// Comprobación barata (sólo lee el JSON, sin PowerShell) que usa la TUI para
/// retirar el aviso en cuanto aparece la clave, sin reiniciar Compass.
pub fn key_is_missing() -> bool {
    match config_file() {
        Some(path) if path.exists() => std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| parse_jsonc(&text).ok())
            .map(|value| !inferx_configured_in(&value))
            .unwrap_or(true),
        // Sin archivo de configuración todavía: OpenCode está instalado pero
        // InferX no existe.
        _ => true,
    }
}

fn reminder_dismissed() -> bool {
    reminder_path().map(|path| path.exists()).unwrap_or(false)
}

/// El usuario pulsó `[x]`: no volver a mostrar el aviso en Compass.
pub fn dismiss_reminder() {
    let Some(path) = reminder_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, "dismissed\n");
}

fn clear_reminder_dismissal() {
    if let Some(path) = reminder_path() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_line_comments() {
        let input = r#"{
            // comentario
            "a": 1,
            "b": "http://no-es-comentario"
        }"#;
        let value: Value = parse_jsonc(input).unwrap();
        assert_eq!(value["a"], 1);
        assert_eq!(value["b"], "http://no-es-comentario");
    }

    #[test]
    fn strips_block_comments_and_trailing_commas() {
        let input = r#"{
            /* bloque */
            "a": 1,
            "b": [1, 2,],
        }"#;
        let value: Value = parse_jsonc(input).unwrap();
        assert_eq!(value["a"], 1);
        assert_eq!(value["b"][1], 2);
    }

    #[test]
    fn merge_adds_provider_and_default_model() {
        let mut value = json!({ "$schema": "https://opencode.ai/config.json" });
        merge_inferx(&mut value, Some("ix_test"));

        let provider = &value["providers"][PROVIDER];
        assert_eq!(provider["name"], "InferX");
        assert_eq!(provider["settings"]["baseURL"], BASE_URL);
        assert_eq!(provider["settings"]["apiKey"], "ix_test");
        assert_eq!(provider["models"][MODEL]["name"], MODEL_NAME);
        assert_eq!(value["model"], format!("{PROVIDER}/{MODEL}"));
    }

    #[test]
    fn merge_preserves_existing_key_and_model_choice() {
        let mut value = json!({
            "model": "otro/modelo",
            "providers": {
                "inferx": {
                    "settings": { "apiKey": "ix_vieja" },
                    "models": { MODEL: { "name": "Nombre propio" } }
                }
            }
        });
        merge_inferx(&mut value, None);

        assert_eq!(value["model"], "otro/modelo");
        assert_eq!(
            value["providers"][PROVIDER]["settings"]["apiKey"],
            "ix_vieja"
        );
        assert_eq!(
            value["providers"][PROVIDER]["models"][MODEL]["name"],
            "Nombre propio"
        );
    }

    #[test]
    fn merge_overwrites_key_when_given() {
        let mut value = json!({
            "providers": { "inferx": { "settings": { "apiKey": "ix_vieja" } } }
        });
        merge_inferx(&mut value, Some("ix_nueva"));
        assert_eq!(
            value["providers"][PROVIDER]["settings"]["apiKey"],
            "ix_nueva"
        );
    }

    #[test]
    fn merge_without_key_leaves_api_key_absent() {
        let mut value = json!({ "$schema": "https://opencode.ai/config.json" });
        merge_inferx(&mut value, None);
        assert!(value["providers"][PROVIDER]["settings"]
            .get("apiKey")
            .is_none());
    }

    #[test]
    fn configured_detects_literal_key() {
        let value = json!({
            "providers": { "inferx": { "settings": { "apiKey": "ix_literal" } } }
        });
        assert!(inferx_configured_in(&value));
    }

    #[test]
    fn placeholder_is_not_configured() {
        let value = json!({
            "providers": { "inferx": { "settings": { "apiKey": PLACEHOLDER } } }
        });
        assert!(!inferx_configured_in(&value));
    }

    #[test]
    fn absent_provider_is_not_configured() {
        let value = json!({ "$schema": "https://opencode.ai/config.json" });
        assert!(!inferx_configured_in(&value));
    }

    #[test]
    fn env_references_are_parsed() {
        assert_eq!(
            env_reference("{env:INFERX_API_KEY}").as_deref(),
            Some("INFERX_API_KEY")
        );
        assert_eq!(env_reference("ix_literal"), None);
    }
}
