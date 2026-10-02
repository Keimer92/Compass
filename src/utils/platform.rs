//! Helpers de portabilidad Windows / Linux.
//!
//! Pop!_OS es Ubuntu-based, así que el soporte Linux es el objetivo. Estos
//! helpers centralizan las tres diferencias que se repitían por todo el crate:
//!
//! 1. **Separador de ruta** — Windows acepta tanto `\` como `/`, y el `std::path`
//!    de Rust normaliza ambos en Windows. Por eso los literales de ruta en todo
//!    el crate usan `/`, que es válido en ambas plataformas. Este módulo sólo
//!    cubre los casos dinámicos.
//! 2. **Extensión de ejecutable** — `adb.exe` en Windows, `adb` en Linux.
//! 3. **Variables de entorno del home** — `USERPROFILE` en Windows, `HOME` en Linux.

use std::path::{Path, PathBuf};

/// ¿Estamos en Windows?
pub const IS_WINDOWS: bool = cfg!(windows);

/// Nombre del ejecutable de `base` con la extensión de la plataforma.
///
/// ```ignore
/// exe_name("adb")        // "adb.exe" en Windows, "adb" en Linux
/// exe_name("gradlew")    // "gradlew.bat" en Windows, "gradlew" en Linux
/// ```
pub fn exe_name(base: &str) -> String {
    if IS_WINDOWS {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// `gradlew.bat` en Windows, `gradlew` en Linux (el wrapper de Gradle).
pub fn gradlew_name() -> &'static str {
    if IS_WINDOWS {
        "gradlew.bat"
    } else {
        "gradlew"
    }
}

/// Directorio home del usuario actual.
///
/// Windows: `USERPROFILE`. Linux: `HOME`.
pub fn home_dir() -> Option<PathBuf> {
    env_path(if IS_WINDOWS { "USERPROFILE" } else { "HOME" })
}

/// `LOCALAPPDATA` en Windows, `XDG_DATA_HOME` (o `$HOME/.local/share`) en Linux.
pub fn data_dir() -> Option<PathBuf> {
    if IS_WINDOWS {
        env_path("LOCALAPPDATA")
    } else if let Some(xdg) = env_path("XDG_DATA_HOME") {
        if xdg.exists() {
            return Some(xdg);
        }
        home_dir().map(|h| h.join(".local").join("share"))
    } else {
        home_dir().map(|h| h.join(".local").join("share"))
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key).ok().filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// NDK version used by Tauri's Android targets.
///
/// Previously duplicated in `context.rs` and `android.rs`; single source now.
pub const NDK_VERSION: &str = "27.0.12077973";

/// Extensiones del artefacto de un build **desktop**, para `reveal_on_success`.
///
/// Windows produce `exe`/`msi`. En Linux el binario no lleva extensión, y Tauri
/// empaqueta además `AppImage` y `deb`; KMP genera una distribución sin
/// extensión. Por eso en Linux se devuelve una lista vacía, que
/// `sh_newest_artifact` interpreta como "cualquier archivo regular" en vez de
/// "no hay filtro" — así el artefacto se revela igual en ambas plataformas.
///
/// Sin esto, `reveal_on_success` nunca encontraba nada en Linux y caía siempre
/// al mensaje "(artefacto no encontrado)".
pub fn desktop_artifact_exts() -> &'static [&'static str] {
    if IS_WINDOWS {
        &["exe", "msi"]
    } else {
        &[]
    }
}

/// Localiza un wrapper de Gradle (`gradlew` o `gradlew.bat`) en `dir`.
///
/// Acepta ambas formas porque un proyecto KMP generado en Windows y luego
/// clonado a Linux conserva `gradlew.bat` pero no `gradlew` (y viceversa).
pub fn find_gradlew(dir: &Path) -> Option<PathBuf> {
    let candidates = ["gradlew.bat", "gradlew"];
    for c in candidates {
        let p = dir.join(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}