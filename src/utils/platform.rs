//! Helpers específicos de Windows y del SDK de Android.

use std::path::{Path, PathBuf};

/// ¿Estamos en Windows?
pub const IS_WINDOWS: bool = cfg!(windows);

/// Nombre del ejecutable de `base` en Windows.
pub fn exe_name(base: &str) -> String {
    format!("{base}.exe")
}

/// Wrapper de Gradle para Windows.
pub fn gradlew_name() -> &'static str {
    "gradlew.bat"
}

/// Nombre del binding nativo que `@tauri-apps/cli` necesita en Windows.
///
/// El CLI de Tauri 2 no es JavaScript puro: delega en un binario que llega como
/// *optional dependency* con nombre por plataforma. Si el `node_modules` se
/// instaló en otro sistema, el de esta plataforma no está y el CLI aborta con
/// `Cannot find native binding` ([npm/cli#4828]).
///
/// La lista no es cerrada: `@tauri-apps/cli` publica un paquete por cada
/// combinación SO/CPU/libc que soporta, y añade otros con cada versión. Estos
/// Para arquitecturas desconocidas se devuelve `None` y la comprobación se salta.
pub fn tauri_native_pkg() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("@tauri-apps/cli-win32-x64-msvc"),
        "aarch64" => Some("@tauri-apps/cli-win32-arm64-msvc"),
        "x86" => Some("@tauri-apps/cli-win32-ia32-msvc"),
        _ => None,
    }
}

/// Directorio home del usuario actual.
///
pub fn home_dir() -> Option<PathBuf> {
    env_path("USERPROFILE")
}

/// Directorio local de datos de Compass.
pub fn data_dir() -> Option<PathBuf> {
    env_path("LOCALAPPDATA")
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key).ok().filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// NDK version used by Tauri's Android targets.
///
/// Previously duplicated in `context.rs` and `android.rs`; single source now.
pub const NDK_VERSION: &str = "27.0.12077973";

/// Extensiones del artefacto de un build **desktop**, para `newest_artifact`.
///
/// Extensiones de paquetes desktop producidos en Windows.
pub fn desktop_artifact_exts() -> &'static [&'static str] {
    &["exe", "msi"]
}

/// Nombre visible de un directorio: el último componente de la ruta (`School
/// Tracker` para `D:\Proyectos\School Tracker`), o la ruta entera si no tiene
/// componentes (p. ej. `C:\`). Se usa como título de las ventanas de terminal.
pub fn dir_display_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

/// Localiza el wrapper de Gradle para Windows.
pub fn find_gradlew(dir: &Path) -> Option<PathBuf> {
    let wrapper = dir.join("gradlew.bat");
    wrapper.exists().then_some(wrapper)
}