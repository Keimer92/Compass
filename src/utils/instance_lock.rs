//! Bloqueo de instancia única por directorio.
//!
//! Si Compass ya está abierto para un directorio y se intenta abrir otra
//! instancia para el mismo directorio, se muestra un banner nativo del sistema
//! avisando que ya está abierto y se sale sin abrir otra instancia.
//!
//! ## Mecanismo
//!
//! Se usa un archivo de bloqueo (lock file) por directorio en el directorio
//! de datos de Compass: `<data_dir>/compass/locks/<hash>.lock`.
//! El archivo contiene el PID del proceso que lo posee.
//!
//! Al arrancar:
//! 1. Se calcula el hash del directorio canónico.
//! 2. Se busca el lock file correspondiente.
//! 3. Si existe y el PID sigue vivo → banner nativo + salida.
//! 4. Si existe pero el PID ya no vive → lock huérfano, se elimina y se continúa.
//! 5. Si no existe → se crea con el PID actual y se continúa.
//!
//! Al salir (normal o por panic), el lock se elimina.

use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::utils::platform;

/// Handle al lock de instancia. Se elimina al hacer drop.
pub struct InstanceLock {
    pub lock_path: PathBuf,
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Calcula un hash estable del directorio para usar como nombre de lock file.
fn dir_hash(dir: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Usamos la ruta canónica para que dos rutas equivalentes den el mismo hash.
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    canonical.to_string_lossy().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Ruta del directorio de locks.
fn locks_dir() -> Option<PathBuf> {
    platform::data_dir().map(|d| d.join("compass").join("locks"))
}

/// Ruta del lock file para un directorio.
fn lock_path_for(dir: &Path) -> Option<PathBuf> {
    locks_dir().map(|d| d.join(format!("{}.lock", dir_hash(dir))))
}

/// Comprueba si un proceso con el PID dado sigue vivo.
fn pid_alive(pid: u32) -> bool {
    if platform::IS_WINDOWS {
        // En Windows usamos `tasklist` para verificar.
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    } else {
        // En Linux, /proc/<pid> existe si el proceso sigue vivo.
        Path::new(&format!("/proc/{}", pid)).exists()
    }
}

/// Muestra un banner nativo del sistema avisando que Compass ya está abierto.
pub fn show_already_open_banner(dir: &Path) {
    let title = "Compass - Ya abierto";
    let msg = format!(
        "Compass ya está abierto para el directorio:\n{}\n\nCierra la instancia existente antes de abrir otra.",
        dir.display()
    );

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        // Convertir strings a wide (UTF-16) para MessageBoxW
        let title_wide: Vec<u16> = OsStr::new(title).encode_wide().chain(std::iter::once(0)).collect();
        let msg_wide: Vec<u16> = OsStr::new(&msg).encode_wide().chain(std::iter::once(0)).collect();

        // MB_OK | MB_ICONINFORMATION | MB_TOPMOST
        const MB_OK: u32 = 0x00000000;
        const MB_ICONINFORMATION: u32 = 0x00000040;
        const MB_TOPMOST: u32 = 0x00040000;

        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, type_: u32) -> i32;
        }

        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                msg_wide.as_ptr(),
                title_wide.as_ptr(),
                MB_OK | MB_ICONINFORMATION | MB_TOPMOST,
            );
        }
        return;
    }

    #[cfg(not(windows))]
    {
        // Linux: intentar notify-send, luego zenity, luego kdialog
        let tools = [
            ("notify-send", vec![title, &msg]),
            ("zenity", vec!["--info", "--title", title, "--text", &msg]),
            ("kdialog", vec!["--msgbox", &msg, "--title", title]),
        ];

        let mut shown = false;
        for (cmd, args) in &tools {
            if std::process::Command::new(cmd)
                .args(args.iter().map(|s| *s))
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                shown = true;
                break;
            }
        }
        if !shown {
            eprintln!("[{}] {}", title, msg);
        }
    }
}

/// Intenta adquirir el lock de instancia para un directorio.
///
/// Devuelve `Ok(Some(lock))` si se adquirió el lock (primera instancia).
/// Devuelve `Ok(None)` si ya hay otra instancia corriendo (y muestra el banner).
/// Devuelve `Err` si hay un error de I/O.
pub fn try_acquire(dir: &Path) -> Result<Option<InstanceLock>, std::io::Error> {
    let lock_path = match lock_path_for(dir) {
        Some(p) => p,
        None => return Ok(Some(InstanceLock { lock_path: PathBuf::new() })),
    };

    // Asegurar que el directorio de locks existe
    if let Some(parent) = lock_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Comprobar si ya hay un lock
    if lock_path.exists() {
        if let Ok(content) = fs::read_to_string(&lock_path) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if pid_alive(pid) {
                    // Ya hay una instancia corriendo
                    show_already_open_banner(dir);
                    return Ok(None);
                }
            }
        }
        // Lock huérfano o corrupto: eliminarlo
        let _ = fs::remove_file(&lock_path);
    }

    // Crear el lock con nuestro PID
    let pid = std::process::id();
    fs::write(&lock_path, pid.to_string())?;

    Ok(Some(InstanceLock { lock_path }))
}
