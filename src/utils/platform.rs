//! Helpers específicos de Windows y del SDK de Android.

use std::path::{Path, PathBuf};

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
    std::env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
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

/// Color de acento de la pestaña para un proyecto.
///
/// Sale del **path canónico y en minúsculas** (Windows no distingue
/// mayúsculas), así que el mismo proyecto conserva su color al reabrirse y dos
/// proyectos distintos abiertos a la vez reciben tonos separados por el hash.
/// La saturación y la luminosidad son fijas para que el texto de la pestaña
/// siga legible en cualquier tema.
pub fn tab_accent(project: &Path) -> [u8; 3] {
    let canonical = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let key = canonical.display().to_string().to_lowercase();
    // Décimas de grado: 3600 cubos reparten mejor que 360 tonos enteros.
    let hue = (fnv1a(key.as_bytes()) % 3600) as f32 / 3600.0;
    hsl_to_rgb(hue, 0.65, 0.45)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_accent_is_stable_per_project() {
        let a = Path::new(r"D:\Proyectos\School Tracker");
        assert_eq!(tab_accent(a), tab_accent(a));
        // En Windows el path no distingue mayúsculas.
        let b = Path::new(r"d:\proyectos\school tracker");
        assert_eq!(tab_accent(a), tab_accent(b));
    }

    #[test]
    fn tab_accent_differs_between_projects() {
        let a = tab_accent(Path::new(r"D:\Proyectos\School Tracker"));
        let b = tab_accent(Path::new(r"D:\Proyectos\SongSync"));
        let c = tab_accent(Path::new(r"D:\Proyectos\Compass"));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn tab_accent_has_visible_saturation() {
        for name in ["School Tracker", "SongSync", "Compass", "Go + Wails v3"] {
            let [r, g, b] = tab_accent(Path::new(name));
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            // Un gris no se distinguiría de otra pestaña.
            assert!(
                (max - min) as u32 > 60,
                "color plano para {name}: {r},{g},{b}"
            );
        }
    }
}
