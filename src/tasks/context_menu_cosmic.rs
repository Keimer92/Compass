//! Menú contextual para **COSMIC Files** (Pop!_OS / COSMIC).
//!
//! Se configura con presets `ContextActionPreset` en el campo `context_actions`
//! de la config de cosmic-files.
//!
//! ## Ruta
//!
//! cosmic-files abre su config con `cosmic_config::Config::new(App::APP_ID,
//! CONFIG_VERSION)` y cosmic-config resuelve eso como:
//!
//! ```text
//! $XDG_CONFIG_HOME/cosmic/<APP_ID>/v<VERSION>/<campo>
//! ```
//!
//! Con `APP_ID = "com.system76.CosmicFiles"` y `CONFIG_VERSION = 1`:
//!
//! ```text
//! ~/.config/cosmic/com.system76.CosmicFiles/v1/context_actions
//! ```
//!
//! El APP_ID lleva punto y mayúsculas (`com.system76.CosmicFiles`), **no** es
//! `cosmic-files`. Escribir en `cosmic-files/` deja el fichero en un directorio
//! que cosmic-files nunca abre: el menú no aparece y no hay ningún error visible
//! en ninguna parte, ni en la app ni en sus logs.
//!
//! ## Formato: RON, no JSON
//!
//! cosmic-config serializa con `ron`, no con JSON. Todos los ficheros de su
//! config están en RON (`favorites`, `tab`, `desktop`…). Por eso este módulo usa
//! el crate `ron` en lugar de `serde_json`.
//!
//! ## Estructura de un preset (`cosmic-files/src/context_action.rs`)
//!
//! ```ron
//! [
//!     (
//!         name: "Vibe Coding: OpenCode",
//!         confirm: false,
//!         selection: Folders,
//!         steps: [
//!             "/ruta/al/wrapper.sh \"%f\"",
//!         ],
//!     ),
//! ]
//! ```
//!
//! - `selection`: enum `Any` | `Files` | `Folders`, con alias de serde en
//!   minúsculas. Serializa en PascalCase; aquí se replican los alias para poder
//!   leer presets escritos a mano.
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

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::utils::platform;

/// Prefijo común de las entradas de Vibe Coding. Debe coincidir con [`VIBE_PREFIX_STR`].
const VIBE_PREFIX: &str = "Vibe Coding: ";

/// Prefijo en tiempo de compilación, para `install`/`uninstall` sin deserializar.
const VIBE_PREFIX_STR: &str = VIBE_PREFIX;

/// Nombre exacto de la entrada que abre el TUI de compass.
///
/// Sin prefijo porque es una entrada única y con nombre reconocible; el
/// emparejamiento es exacto (ver [`Preset::is_ours`]) para no capturar presets
/// del usuario que empiecen por "Compass".
const COMPASS_NAME: &str = "Compass";

/// A qué se aplica el preset.
///
/// Réplica de `ContextActionSelection` en `cosmic-files/src/context_action.rs`.
/// El enum real usa `#[serde(alias = "any")]` (alias, no `rename_all`), así que
/// serializa como `Any`/`Files`/`Folders` pero acepta también minúsculas; aquí se
/// replican los alias para poder leer presets escritos a mano.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Selection {
    #[default]
    #[serde(alias = "any")]
    Any,
    #[serde(alias = "files")]
    Files,
    #[serde(alias = "folders")]
    Folders,
}

/// Un preset de cosmic-files.
///
/// Réplica de `ContextActionPreset` (`cosmic-files/src/context_action.rs`). El
/// struct real lleva `#[serde(default)]`, de modo que un preset con campos
/// ausentes se completa con valores por defecto en vez de fallar.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Preset {
    name: String,
    confirm: bool,
    selection: Selection,
    steps: Vec<String>,
}

impl Preset {
    fn vibe(title: &str, wrapper: &PathBuf) -> Self {
        Preset {
            name: format!("{VIBE_PREFIX}{title}"),
            steps: vec![exec_step(wrapper)],
            ..Default::default()
        }
        .with_selection(Selection::Folders)
    }

    /// Entrada principal: abrir el menú TUI de compass en la carpeta.
    ///
    /// `selection: Any` (no `Folders`) a propósito: el wrapper resuelve un
    /// fichero a su directorio contenedor, así que funciona igual seleccionando
    /// un archivo que una carpeta. En Windows el registro lo limitaba a
    /// `Directory`, lo que hacía inútil la entrada sobre un archivo suelto.
    fn compass(wrapper: &PathBuf) -> Self {
        Preset {
            name: COMPASS_NAME.to_string(),
            steps: vec![exec_step(wrapper)],
            ..Default::default()
        }
        .with_selection(Selection::Any)
    }

    fn with_selection(mut self, selection: Selection) -> Self {
        self.selection = selection;
        self
    }

    /// ¿Es una entrada instalada por nosotros?
    ///
    /// Cubre las de Vibe Coding (por prefijo) y la de Compass (por nombre
    /// exacto). Se usa en `install`/`uninstall` para preservar las del usuario.
    fn is_ours(&self) -> bool {
        self.name.starts_with(VIBE_PREFIX_STR) || self.name == COMPASS_NAME || self.name == "Code Runner"
    }
}

/// Construye un paso `Exec=` de freedesktop que invoca `wrapper` con la ruta
/// seleccionada en `%f`.
///
/// El `%f` va entrecomillado porque se sustituye literalmente: sin comillas, una
/// ruta con espacios rompería el tokenizado del Exec.
fn exec_step(wrapper: &Path) -> String {
    format!("{} \"%f\"", wrapper.display())
}

/// Directorio de los wrappers `vibe-*` que lanzan cada CLI en la carpeta elegida.
///
/// Existe un wrapper por herramienta en vez de incrustar `sh -c "cd %f && ..."`
/// en el Exec porque así el `cd` y el quoting quedan en un archivo que podemos
/// escapar una sola vez, en vez de pelear con el tokenizer de `Exec=`.
fn wrapper_dir() -> Option<PathBuf> {
    Some(
        platform::data_dir()?
            .join("compass")
            .join("vibe"),
    )
}

/// `App::APP_ID` de cosmic-files (`cosmic-files/src/app.rs`).
const COSMIC_FILES_APP_ID: &str = "com.system76.CosmicFiles";

/// `CONFIG_VERSION` de cosmic-files (`cosmic-files/src/config.rs`).
const COSMIC_FILES_CONFIG_VERSION: u64 = 1;

/// Ruta del fichero de `context_actions` de cosmic-files.
///
/// cosmic-files abre su config con `cosmic_config::Config::new(App::APP_ID,
/// CONFIG_VERSION)`, y cosmic-config lo resuelve como
/// `$XDG_CONFIG_HOME/cosmic/<APP_ID>/v<VERSION>/<campo>`. O sea:
///
/// ```text
/// ~/.config/cosmic/com.system76.CosmicFiles/v1/context_actions
/// ```
///
/// El APP_ID es `com.system76.CosmicFiles` (con el punto y las mayúsculas), no
/// `cosmic-files`: escribir en `cosmic-files/` deja la config en un directorio
/// que cosmic-files nunca abre, y el menú no aparece sin ningún error visible.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| platform::home_dir().map(|h| h.join(".config")))?;
    Some(
        base.join("cosmic")
            .join(COSMIC_FILES_APP_ID)
            .join(format!("v{COSMIC_FILES_CONFIG_VERSION}"))
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

/// Nombre del wrapper de la entrada principal de Compass.
const COMPASS_WRAPPER: &str = "compass-open.sh";

/// Ruta del ejecutable de compass.
///
/// `current_exe()` es lo correcto porque cosmic-files puede lanzarlo desde
/// cualquier CWD y sin PATH de sesión (mismo motivo por el que el `.desktop`
/// lleva la ruta absoluta).
fn compass_exe() -> Result<PathBuf> {
    std::env::current_exe().context("No se pudo determinar la ruta de compass")
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

/// Preámbulo común a todos los wrappers.
///
/// `$1` es la ruta que cosmic-files sustituyó en `%f`. Puede ser un fichero o una
/// carpeta (según `selection`), así que se normaliza a directorio: sin esto,
/// `cd` a un fichero falla y el TUI no llega a abrirse.
const WRAPPER_PREAMBLE: &str = r#"#!/usr/bin/env sh
# Generado por compass. NO editar: se regenera al reinstalar.
# Abre su propia terminal porque cosmic-files no asigna un tty, y sin tty
# las TUI no son interactivas.
set -e
target="${1:-$(pwd)}"
# Si `%f` era un fichero, se trabaja en su carpeta contenedor.
if [ -f "$target" ]; then
    target="$(dirname "$target")"
fi
[ -d "$target" ] || { echo "No existe: $target" >&2; exit 1; }
# `export` es imprescindible: el comando se ejecuta en un `sh -lc` hijo, y una
# variable sin exportar llega ahí como cadena vacía. Sin esto, `cd "$target"`
# hace `cd ""`, que en dash no falla sino que se queda en el directorio actual:
# la TUI se abriría en la carpeta desde la que se lanzó, no en la elegida.
export target
"#;

/// Escribe (o sobreescribe) el wrapper de una herramienta de Vibe Coding.
///
/// El wrapper abre una terminal en la carpeta recibida. `read -r` al final
/// mantiene la ventana abierta para poder leer la salida, igual que las
/// ventanas de dev/build. Sin emulador instalado cae a ejecución directa, con un
/// aviso en el propio wrapper.
fn write_wrapper(dir: &std::path::Path, title: &str, cli: &str) -> Result<PathBuf> {
    let path = dir.join(wrapper_name(title));
    write_terminal_script(&path, title, &format!("exec {cli}"))?;
    Ok(path)
}

/// Escribe el wrapper de la entrada principal: abre el TUI de compass.
///
/// No lleva `--pick`: la carpeta viene dada. Si esa carpeta no es la raíz de un
/// proyecto, `compass` cae por su cuenta al selector de proyecto empezando
/// por ella (en `main.rs`, `detect()` falla sin `--task` ⇒ `pick_project_root`).
fn write_compass_wrapper(dir: &std::path::Path) -> Result<PathBuf> {
    let path = dir.join(COMPASS_WRAPPER);
    let exe = compass_exe()?;
    // Ruta absoluta entrecomillada con shlex-style simple; se pasa como
    // argumento de `--path` sin quoting POSIX porque ya va dentro de una
    // cadena que `sh -lc` re-evalúa con comillas dobles.
    let cmd = format!("exec {} --path \"$target\"", shell_quote(&exe.display().to_string()));
    write_terminal_script(&path, COMPASS_NAME, &cmd)?;
    Ok(path)
}

/// Genera un wrapper que abre el comando `cmd` en una ventana de terminal.
fn write_terminal_script(path: &std::path::Path, title: &str, cmd: &str) -> Result<()> {
    // `cmd` va como un solo argumento del shell embebido, no partido en tokens:
    // `opencode --auto` debe llegarle a `sh -lc` como una sola cadena. Por eso
    // se cita en el argv y se deja sin comillas dentro.
    let script = match crate::utils::cmd::terminal_launch_shell(
        title,
        &format!(
            r#"cd "$target" || exit 1; {cmd}; rc=$?; echo; \
               if [ $rc -ne 0 ]; then echo "[ERROR] {title} terminó con código $rc."; fi; \
               read -r"#
        ),
    ) {
        Some(cmd_line) => format!("{WRAPPER_PREAMBLE}{cmd_line}\n"),
        None => format!(
            "{WRAPPER_PREAMBLE}# [WARN] No se encontró emulador de terminal: se lanza sin\n\
             #        tty, así que la TUI no será interactiva.\n\
             cd \"$target\"\n\
             {cmd}\n"
        ),
    };

    std::fs::write(&path, script)
        .with_context(|| format!("No se pudo escribir {}", path.display()))?;
    set_executable(&path)?;
    Ok(())
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
///
/// El formato es **RON**, no JSON: cosmic-config serializa con `ron`, igual que
/// el resto de su configuración (`favorites`, `tab`, `desktop`…).
fn load_presets(path: &std::path::Path) -> Vec<Preset> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    ron::from_str(&text).unwrap_or_default()
}

/// Escribe los presets en RON.
///
/// Usa `to_string_pretty` con sangría de 4 espacios para coincidir con el
/// formato que produce cosmic-config, en vez del compacto de `ron` por defecto.
fn save_presets(path: &std::path::Path, presets: &[Preset]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("No se pudo crear {}", parent.display()))?;
    }
    let r = ron::ser::to_string_pretty(presets, ron::ser::PrettyConfig::new())
        .context("No se pudo serializar los presets")?;
    std::fs::write(path, r)
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
             Instala al menos una (opencode, kilo, cline, agy o hermes) y vuelve a ejecutar\n\
             'compass --task opencode-install'."
        );
    }

    println!("[INFO] Instalando entradas del menú contextual de COSMIC Files...");
    if !missing.is_empty() {
        println!("[WARN] No se encontraron: {}", missing.join(", "));
        println!("[WARN] Sólo se instalarán las que sí están disponibles.");
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("No se pudo crear {}", dir.display()))?;

    // 1. Wrappers ejecutables. La entrada de Compass va primera: es la
    //    principal, y las de Vibe Coding quedan agrupadas debajo por prefijo.
    let mut installed = Vec::new();

    let cr_wrapper = write_compass_wrapper(&dir)?;
    println!("  ✔ wrapper {}", cr_wrapper.display());
    installed.push(Preset::compass(&cr_wrapper));

    for (title, cli) in &wanted {
        let p = write_wrapper(&dir, title, cli)?;
        println!("  ✔ wrapper {}", p.display());
        installed.push(Preset::vibe(title, &p));
    }

    // 2. Merge en la config, preservando los presets que ya tuviera el usuario.
    let mut presets = load_presets(&cfg);
    let user_count = presets.iter().filter(|p| !p.is_ours()).count();

    presets.retain(|p| !p.is_ours());
    presets.extend(installed);

    // Respaldo antes de escribir: si el RON quedó mal, el usuario puede
    // restaurarlo a mano.
    if cfg.exists() {
        let bak = cfg.with_extension("ron.compass-bak");
        let _ = std::fs::copy(&cfg, &bak);
        println!("[INFO] Respaldo de la config previa: {}", bak.display());
    }

    save_presets(&cfg, &presets)?;

    println!();
    println!("[OK] Menú contextual instalado en COSMIC Files.");
    println!("  Config: {}", cfg.display());
    println!("  Presets del usuario preservados: {user_count}");
    println!();
    println!("  Entradas (planas, COSMIC no soporta submenús anidados):");
    println!("    {COMPASS_NAME}                    ← en carpetas y en archivos");
    for (title, _) in &wanted {
        println!("    {VIBE_PREFIX}{title}");
    }
    println!();
    println!("[INFO] Cierra COSMIC Files por completo y vuelve a abrirlo.");
    println!("[INFO] Las entradas sólo salen con algo seleccionado: cosmic-files no");
    println!("       las añade al menú del clic en el fondo de la carpeta.");
    Ok(())
}

/// Elimina únicamente nuestros presets, dejando intactos los del usuario.
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
    presets.retain(|p| !p.is_ours());
    let removed = before - presets.len();

    if removed == 0 {
        println!("[OK] No había entradas de compass instaladas.");
        return Ok(());
    }

    save_presets(&cfg, &presets)?;
    if let Some(dir) = wrapper_dir() {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    println!("[OK] {removed} entradas eliminadas de COSMIC Files.");
    println!("[INFO] Cierra COSMIC Files por completo y vuelve a abrirlo.");
    Ok(())
}

/// ¿Están instaladas nuestras entradas?
pub fn is_installed() -> bool {
    config_path()
        .map(|p| load_presets(&p).iter().any(|x| x.is_ours()))
        .unwrap_or(false)
}

/// ¿Hay alguna entrada de Vibe Coding instalada?
///
/// Distinto de [`is_installed`], que además cuenta la entrada de Compass.
/// El menú interactivo los muestra como dos interruptores separados.
pub fn has_vibe_entries() -> bool {
    config_path()
        .map(|p| load_presets(&p).iter().any(|x| x.name.starts_with(VIBE_PREFIX_STR)))
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
    VibeToolLinux { title: "Kiro CLI", cli: "kiro-cli chat" },
    VibeToolLinux { title: "Hermes", cli: "hermes" },
];

struct VibeToolLinux {
    title: &'static str,
    cli: &'static str,
}
