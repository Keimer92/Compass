//! Menú contextual para **COSMIC Files** (Pop!_OS / COSMIC).
//!
//! COSMIC 1.0.10 (mayo 2026) añadió soporte de *user-defined context menu
//! actions*. Se configuran como presets `ContextActionPreset` dentro del campo
//! `context_actions` de la config de cosmic-files (esquema v1), que sigue el
//! patrón de `cosmic-config`:
//!
//! ```text
//! $XDG_CONFIG_HOME/cosmic/cosmic-files/v1/context_actions
//! ```
//!
//! Estructura de un preset (`cosmic-files/src/context_action.rs`):
//!
//! ```json
//! { "name": "...", "confirm": false, "selection": "folders", "steps": ["..."] }
//! ```
//!
//! - `selection`: `any` | `files` | `folders` (alias de serde para Any/Files/Folders)
//! - `steps`: cada elemento se parsea con `mime_app::exec_to_command`, es decir
//!   **sintaxis `Exec=` de freedesktop**. El código de campo `%f` se sustituye
//!   por la ruta de la carpeta, y debe ir entrecomillado para sobrevivir a rutas
//!   con espacios.
//!
//! ## Limitación conocida: sin submenús anidados
//!
//! `ContextActionPreset` **no tiene campo de agrupación**, así que no se puede
//! replicar el cascada "Vibe Coding ▸ OpenCode / Kilo Code / Cline /
//! Antigravity" de Windows. Las entradas quedan planas en el menú. Para que sigan
//! siendo reconocibles y reversibles, todas se prefijan con `"Vibe Coding: "`,
//! que es el criterio que usa [`uninstall`] para borrar sólo las nuestras sin
//! tocar las del usuario.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::utils::platform;

/// Prefijo común de nuestras entradas. Debe coincidir con [`VIBE_PREFIX_STR`].
const VIBE_PREFIX: &str = "Vibe Coding: ";

/// Prefijo en tiempo de compilación, para `install`/`uninstall` sin deserializar.
const VIBE_PREFIX_STR: &str = VIBE_PREFIX;

/// Un preset de cosmic-files. Sólo los campos que realmente usa el deserializador
/// de cosmic-files; `#[serde(default)]` en su struct hace que el resto sea opcional.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Preset {
    name: String,
    #[serde(default)]
    confirm: bool,
    #[serde(default = "any_selection")]
    selection: String,
    #[serde(default)]
    steps: Vec<String>,
}

fn any_selection() -> String {
    "any".to_string()
}

impl Preset {
    fn vibe(title: &str, wrapper: &PathBuf) -> Self {
        // El campo `%f` va entrecomillado: es un code field de freedesktop y se
        // sustituye literalmente, así que sin comillas una ruta con espacios
        // rompería el tokenizado del Exec.
        Preset {
            name: format!("{VIBE_PREFIX}{title}"),
            confirm: false,
            selection: "folders".to_string(),
            steps: vec![format!("{} \"%f\"", wrapper.display())],
        }
    }

    fn is_vibe(&self) -> bool {
        self.name.starts_with(VIBE_PREFIX_STR)
    }
}

/// Directorio de los wrappers `vibe-*` que lanzan cada CLI en la carpeta elegida.
///
/// Existe un wrapper por herramienta en vez de incrustar `sh -c "cd %f && ..."`
/// en el Exec porque así el `cd` y el quoting quedan en un archivo que podemos
/// escapar una sola vez, en vez de pelear con el tokenizer de `Exec=`.
fn wrapper_dir() -> Option<PathBuf> {
    Some(
        platform::data_dir()?
            .join("code-runner")
            .join("vibe"),
    )
}

/// Ruta de la config de context actions de cosmic-files.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| platform::home_dir().map(|h| h.join(".config")))?;
    Some(
        base.join("cosmic")
            .join("cosmic-files")
            .join("v1")
            .join("context_actions"),
    )
}

/// Nombre del wrapper para una herramienta, derivado de su título.
fn wrapper_name(title: &str) -> String {
    let slug: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    format!("vibe-{slug}.sh")
}

/// Detecta si cosmic-files está instalado (para no escribir config en otra distro).
fn cosmic_files_installed() -> bool {
    cmd_has("cosmic-files")
}

fn cmd_has(bin: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v -- {}", shell_quote(bin))])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// ¿Está disponible el binario de una línea de comando como `opencode --auto`?
///
/// Sólo se mira el primer token: los flags (`--auto`, `-i`) no son ejecutables.
fn cli_available(cli: &str) -> bool {
    cli.split_whitespace()
        .next()
        .map(cmd_has)
        .unwrap_or(false)
}

/// Escribe (o sobreescribe) el wrapper de una herramienta.
///
/// El wrapper recibe la carpeta como `$1` y **abre su propia ventana de
/// terminal** en ella. Esto no es opcional: cosmic-files lanza el paso sin tty
/// (stdin/stdout heredados a un pipe), y estas CLIs son TUI. Con un `exec
/// opencode` directo, OpenCode y cline escriben sus secuencias ANSI al pipe y no
/// queda una sesión interactiva usable.
///
/// `read -r` al final mantiene la ventana abierta para poder leer la salida,
/// igual que las ventanas de dev/build. Sin emulador instalado cae a ejecución
/// directa, con un aviso en el propio wrapper.
fn write_wrapper(dir: &std::path::Path, title: &str, cli: &str) -> Result<PathBuf> {
    let path = dir.join(wrapper_name(title));

    // `cli` va como un solo argumento del shell embebido, no partido en tokens:
    // `opencode --auto` debe llegarle a `sh -lc` como una sola cadena de
    // comandos. Por eso se cita aquí y dentro se deja sin comillas.
    let script = match crate::utils::cmd::terminal_prefix(
        title,
        &format!(
            r#"cd "$target" || exit 1; {cli}; rc=$?; echo; \
               if [ $rc -ne 0 ]; then echo "[ERROR] {title} terminó con código $rc."; fi; \
               read -r"#
        ),
    ) {
        Some(argv) => {
            let cmd_line = argv
                .iter()
                .map(|a| shell_quote(a))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "#!/usr/bin/env sh\n\
                 # Generado por code-runner. Abre {title} en la carpeta recibida.\n\
                 # Abre su propia terminal porque cosmic-files no asigna un tty,\n\
                 # y sin tty la TUI de la CLI no es interactiva.\n\
                 set -e\n\
                 target=\"${{1:-$(pwd)}}\"\n\
                 [ -d \"$target\" ] || {{ echo \"No existe: $target\" >&2; exit 1; }}\n\
                 exec {cmd_line}\n"
            )
        }
        None => format!(
            "#!/usr/bin/env sh\n\
             # Generado por code-runner. Lanza {title} en la carpeta recibida.\n\
             # [WARN] No se encontró emulador de terminal, así que la CLI se lanza\n\
             #        sin tty y su TUI no será interactiva.\n\
             set -e\n\
             target=\"${{1:-$(pwd)}}\"\n\
             cd \"$target\" || exit 1\n\
             exec {cli}\n"
        ),
    };

    std::fs::write(&path, script)
        .with_context(|| format!("No se pudo escribir {}", path.display()))?;
    set_executable(&path)?;
    Ok(path)
}

/// Entrecomilla `s` para POSIX `sh` (comillas simples, escapando la interna).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn set_executable(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(path)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(path, perm)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Lee los presets actuales. Un archivo ausente o corrupto se trata como lista
/// vacía, pero se respalda antes de escribir para no perder datos del usuario.
fn load_presets(path: &std::path::Path) -> Vec<Preset> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_presets(path: &std::path::Path, presets: &[Preset]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("No se pudo crear {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(presets)
        .context("No se pudo serializar los presets")?;
    std::fs::write(path, json)
        .with_context(|| format!("No se pudo escribir {}", path.display()))?;
    Ok(())
}

/// Instala las entradas de Vibe Coding en el menú contextual de COSMIC Files.
pub fn install() -> Result<()> {
    if !cosmic_files_installed() {
        anyhow::bail!(
            "[ERROR] No se encontró `cosmic-files`.\n\
             Esta función sólo aplica a Pop!_OS con COSMIC.\n\
             En otro sistema, el menú contextual no está soportado."
        );
    }

    let dir = wrapper_dir().context("No se pudo determinar el directorio de datos")?;
    let cfg = config_path().context("No se pudo determinar XDG_CONFIG_HOME")?;

    // Sólo las CLIs realmente disponibles: un preset cuyo comando no existe
    // abre una ventana que muere al instante, que es peor que no mostrarla.
    let wanted: Vec<(&str, &str)> = VIBE_TOOLS
        .iter()
        .filter(|t| cli_available(t.cli))
        .map(|t| (t.title, t.cli))
        .collect();

    let missing: Vec<&str> = VIBE_TOOLS
        .iter()
        .filter(|t| !cli_available(t.cli))
        .map(|t| t.title)
        .collect();

    if wanted.is_empty() {
        anyhow::bail!(
            "[ERROR] No se encontró ninguna CLI de Vibe Coding en el PATH.\n\
             Instala al menos una (opencode, kilo, cline o agy) y vuelve a ejecutar\n\
             'code-runner --task opencode-install'."
        );
    }

    println!("[INFO] Instalando entradas de Vibe Coding en COSMIC Files...");
    if !missing.is_empty() {
        println!("[WARN] No se encontraron: {}", missing.join(", "));
        println!("[WARN] Sólo se instalarán las que sí están disponibles.");
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("No se pudo crear {}", dir.display()))?;

    // 1. Wrappers ejecutables.
    let mut installed = Vec::new();
    for (title, cli) in &wanted {
        let p = write_wrapper(&dir, title, cli)?;
        println!("  ✔ wrapper {}", p.display());
        installed.push(Preset::vibe(title, &p));
    }

    // 2. Merge en la config, preservando los presets que ya tuviera el usuario.
    let mut presets = load_presets(&cfg);
    let user_count = presets.iter().filter(|p| !p.is_vibe()).count();

    presets.retain(|p| !p.is_vibe());
    presets.extend(installed);

    // Respaldo antes de escribir: si el JSON quedó mal, el usuario puede
    // restaurarlo a mano.
    if cfg.exists() {
        let bak = cfg.with_extension("json.code-runner-bak");
        let _ = std::fs::copy(&cfg, &bak);
        println!("[INFO] Respaldo de la config previa: {}", bak.display());
    }

    save_presets(&cfg, &presets)?;

    println!();
    println!("[OK] Menú contextual de Vibe Coding instalado en COSMIC Files.");
    println!("  Config: {}", cfg.display());
    println!("  Presets del usuario preservados: {user_count}");
    println!();
    println!("  Entradas (planas, COSMIC no soporta submenús anidados):");
    for (title, _) in &wanted {
        println!("    {} {}", VIBE_PREFIX, title);
    }
    println!();
    println!("[INFO] Cierra COSMIC Files por completo y vuelve a abrirlo.");
    println!("[INFO] Si no aparecen, confirma tu versión: se requiere");
    println!("       COSMIC >= 1.0.10 para context actions de usuario.");
    Ok(())
}

/// Elimina únicamente los presets de Vibe Coding, dejando intactos los del usuario.
pub fn uninstall() -> Result<()> {
    let cfg = match config_path() {
        Some(p) => p,
        None => {
            println!("[WARN] No se pudo determinar XDG_CONFIG_HOME.");
            return Ok(());
        }
    };

    let mut presets = load_presets(&cfg);
    let before = presets.len();
    presets.retain(|p| !p.is_vibe());
    let removed = before - presets.len();

    if removed == 0 {
        println!("[OK] No había entradas de Vibe Coding instaladas.");
        return Ok(());
    }

    save_presets(&cfg, &presets)?;
    if let Some(dir) = wrapper_dir() {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    println!("[OK] {removed} entradas de Vibe Coding eliminadas de COSMIC Files.");
    println!("[INFO] Cierra COSMIC Files por completo y vuelve a abrirlo.");
    Ok(())
}

/// ¿Están instaladas nuestras entradas?
pub fn is_installed() -> bool {
    config_path()
        .map(|p| load_presets(&p).iter().any(|x| x.is_vibe()))
        .unwrap_or(false)
}

/// Definición de herramientas de Vibe Coding.
///
/// Duplica `VIBE_TOOLS` de `context_menu.rs` a propósito: ese array existe dentro
/// de un módulo gateado a Windows, y sus campos (`key`) son claves de registro que
/// no significan nada aquí. Si se unifica, conviene subirlo a un módulo común.
const VIBE_TOOLS: &[VibeToolLinux] = &[
    VibeToolLinux { title: "OpenCode", cli: "opencode --auto" },
    VibeToolLinux { title: "Kilo Code", cli: "kilo" },
    VibeToolLinux { title: "Cline", cli: "cline -i" },
    VibeToolLinux { title: "Antigravity", cli: "agy" },
];

struct VibeToolLinux {
    title: &'static str,
    cli: &'static str,
}
