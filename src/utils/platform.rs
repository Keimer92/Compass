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
/// mayúsculas), así que el mismo proyecto conserva su color al reabrirse. La
/// saturación y la luminosidad son fijas para que el texto de la pestaña siga
/// legible en cualquier tema.
///
/// El tono se cuantiza a `TONES` posiciones de la rueda y se gira según la
/// instancia de Compass que lo pide: así dos proyectos cuyos hashes caían cerca
/// (p. ej. `#28bd2b` y `#49bd28`, ambos verdes) y dos instancias del *mismo*
/// proyecto se ven siempre como colores distintos.
pub fn tab_accent(project: &Path) -> [u8; 3] {
    let hue = accent_hue(fnv1a(project_key(project).as_bytes()), instance_index());
    hsl_to_rgb(hue / 360.0, 0.65, 0.45)
}

/// Clave cromática del proyecto: su path canónico, en minúsculas.
fn project_key(project: &Path) -> String {
    project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf())
        .display()
        .to_string()
        .to_lowercase()
}

/// Posiciones de la rueda de tono. 12 → 30° de separación, discernible a un
/// golpe de vista en la pestaña.
const TONES: u64 = 12;
/// Giro entre instancias: el ángulo áureo de la rueda. Como 360/137,508 no es
/// entero, ninguna cantidad razonable de instancias repite tono.
const GOLDEN: f32 = 137.508;

/// Tono (grados) del acento de una instancia.
fn accent_hue(hash: u64, instance: usize) -> f32 {
    let base = (hash % TONES) as f32 * (360.0 / TONES as f32);
    (base + instance as f32 * GOLDEN) % 360.0
}

/// Índice de este proceso entre los Compass que están corriendo a la vez.
///
/// Se ordena por PID y se cuenta cuántos son anteriores: el primer Compass de la
/// sesión devuelve 0 y conserva por tanto el tono de su proyecto. Se calcula una
/// sola vez por proceso —si otro Compass se cierra mientras este vive, su pestaña
/// no debe cambiar de color a media sesión.
fn instance_index() -> usize {
    static INDEX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *INDEX.get_or_init(|| {
        let myself = std::process::id();
        live_pids_named_self()
            .into_iter()
            .filter(|pid| *pid < myself)
            .count()
    })
}

#[cfg(windows)]
fn live_pids_named_self() -> Vec<u32> {
    let Some(name) = own_exe_name() else {
        return Vec::new();
    };
    unsafe {
        let snap = win::CreateToolhelp32Snapshot(win::TH32CS_SNAPPROCESS, 0);
        if snap == win::INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut pids = Vec::new();
        // Estructura POD: la API pide el tamaño en `dw_size` y el resto a cero.
        let mut entry: win::PROCESSENTRY32W = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<win::PROCESSENTRY32W>() as u32;
        let mut step = win::Process32FirstW(snap, &mut entry) != 0;
        while step {
            if wide_to_lower(&entry.sz_exe_file) == name {
                pids.push(entry.th32_process_id);
            }
            step = win::Process32NextW(snap, &mut entry) != 0;
        }
        win::CloseHandle(snap);
        pids
    }
}

#[cfg(not(windows))]
fn live_pids_named_self() -> Vec<u32> {
    Vec::new()
}

/// Nombre en minúsculas de este ejecutable (`compass.exe`), con el que se
/// comparan las entradas del snapshot.
fn own_exe_name() -> Option<String> {
    std::env::current_exe()
        .ok()?
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
}

#[cfg(windows)]
fn wide_to_lower(field: &[u16]) -> String {
    let end = field
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(field.len());
    String::from_utf16_lossy(&field[..end]).to_lowercase()
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    pub const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;

    /// `dw_size` va primero y el compilador inserta el relleno de 64 bits antes
    /// de `th32_module_id`, así que el layout coincide con el que espera la API.
    #[repr(C)]
    pub struct PROCESSENTRY32W {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_process_id: u32,
        pub th32_default_heap_id: usize,
        pub th32_module_id: u32,
        pub cnt_threads: u32,
        pub th32_parent_process_id: u32,
        pub pc_pri_class_base: i32,
        pub dw_flags: u32,
        pub sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
        pub fn Process32FirstW(snap: *mut c_void, entry: *mut PROCESSENTRY32W) -> i32;
        pub fn Process32NextW(snap: *mut c_void, entry: *mut PROCESSENTRY32W) -> i32;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
        pub fn GetConsoleTitleW(buffer: *mut u16, size: u32) -> u32;
    }
}

/// Título **real** de la consola del proceso, o `None` si no hay consola (salida
/// redirigida, tests) o la API falla.
///
/// Sirve para detectar que otro proceso lo ha sobrescrito: una pestaña ConPTY
/// interna propaga su título a la consola del host (ver `cmd::set_console_title`)
/// y el `OpenConsole` elevado le antepone `Administrator: `.
#[cfg(windows)]
pub fn console_title() -> Option<String> {
    const CAP: usize = 256;
    let mut buf = vec![0u16; CAP];
    let copied = unsafe { win::GetConsoleTitleW(buf.as_mut_ptr(), CAP as u32) };
    if copied == 0 {
        return None;
    }
    // La API copia el carácter nulo; si el título cabía justo, `copied` puede
    // exceder el búfer, así que se recorta antes de convertir.
    let len = (copied as usize).saturating_sub(1).min(CAP);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(not(windows))]
pub fn console_title() -> Option<String> {
    None
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

    /// Tono (grados) que `tab_accent` usarían para `p` si fuera la primera
    /// instancia de la sesión.
    fn hue_de(p: &Path) -> f32 {
        accent_hue(fnv1a(project_key(p).as_bytes()), 0)
    }

    /// Distancia corta entre dos tonos de la rueda.
    fn arco(a: f32, b: f32) -> f32 {
        let d = (a - b).abs() % 360.0;
        d.min(360.0 - d)
    }

    #[test]
    fn tab_accent_is_stable_per_project() {
        let a = Path::new(r"D:\Proyectos\School Tracker");
        assert_eq!(tab_accent(a), tab_accent(a));
        // En Windows el path no distingue mayúsculas.
        let b = Path::new(r"d:\proyectos\school tracker");
        assert_eq!(tab_accent(a), tab_accent(b));
    }

    /// Los tonos caen en la rejilla de 12: ya no hay dos proyectos a 5° uno de
    /// otro, que es lo que hacía que todas las pestañas se vieran «el mismo
    /// verde». O comparten tono (y entonces los separa el giro por instancia) o
    /// están a 30° o más.
    #[test]
    fn project_tones_land_on_the_twelve_tone_grid() {
        for name in [
            "School Tracker",
            "SongSync",
            "Compass",
            "Go + Wails v3",
            "GymPro",
            "Bingoku",
            "AirLyrics",
            "Activator",
        ] {
            let hue = hue_de(Path::new(&format!(r"D:\Proyectos\{name}")));
            assert!(
                (hue % 30.0).abs() < 1e-3 || (hue % 30.0 - 30.0).abs() < 1e-3,
                "{name} se salió de la rejilla: {hue}"
            );
        }
    }

    #[test]
    fn tones_are_grid_spaced_between_projects() {
        let rutas = [
            "School Tracker",
            "SongSync",
            "Compass",
            "GymPro",
            "Bingoku",
            "AirLyrics",
            "Activator",
        ]
        .map(|n| PathBuf::from(format!(r"D:\Proyectos\{n}")));
        for (i, a) in rutas.iter().enumerate() {
            for b in rutas.iter().skip(i + 1) {
                let d = arco(hue_de(a), hue_de(b));
                assert!(
                    d < 1e-3 || d >= 30.0 - 1e-3,
                    "{a:?} y {b:?} caen a {d:.1}° el uno del otro"
                );
            }
        }
    }

    /// Dos instancias abiertas a la vez nunca heredan el mismo color, aunque el
    /// proyecto sea idéntico: el giro áureo las separa.
    #[test]
    fn live_instances_never_share_accent() {
        // El mismo proyecto, seis instancias seguidas: ninguna cae cerca.
        let clave = fnv1a(b"d:\\proyectos\\compass");
        let tonos: Vec<f32> = (0..7).map(|k| accent_hue(clave, k)).collect();
        for (i, a) in tonos.iter().enumerate() {
            for b in tonos.iter().skip(i + 1) {
                assert!(
                    arco(*a, *b) >= 30.0,
                    "instancias del mismo proyecto a {:.1}°",
                    arco(*a, *b)
                );
            }
        }

        // Proyectos distintos, cada uno en su instancia: nunca el mismo tono.
        let rutas = [
            "School Tracker",
            "SongSync",
            "Compass",
            "GymPro",
            "Bingoku",
            "AirLyrics",
        ]
        .iter()
        .enumerate()
        .map(|(k, n)| (k, PathBuf::from(format!(r"D:\Proyectos\{n}"))))
        .map(|(k, p)| accent_hue(fnv1a(project_key(p.as_path()).as_bytes()), k))
        .collect::<Vec<_>>();
        for (i, a) in rutas.iter().enumerate() {
            for b in rutas.iter().skip(i + 1) {
                assert!(
                    arco(*a, *b) >= 10.0,
                    "dos instancias de proyectos distintos caen a {:.1}°",
                    arco(*a, *b)
                );
            }
        }
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
