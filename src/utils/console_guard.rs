//! Guarda de cierre de la ventana anfitriona.
//!
//! Compass corre dentro de la consola/terminal del usuario y sus pestañas
//! internas son procesos hijos. Si esa ventana se cierra (la `×` de conhost,
//! la pestaña de Windows Terminal, un cierre de sesión…), el proceso muere sin
//! pasar por el menú y los árboles de las pestañas quedarían huérfanos.
//!
//! - **conhost**: mientras hay pestañas internas, el elemento `SC_CLOSE` del
//!   menú del sistema se deshabilita, así que la `×` no cierra la ventana. Se
//!   vuelve a habilitar al cerrar la última pestaña. La salida pasa a ser la
//!   del menú (`Esc` → aviso → `Enter`).
//! - **Windows Terminal**: no hay forma de vetar el cierre de una pestaña desde
//!   la app (microsoft/terminal#6549); el host cierra la ConPTY igual. Aquí el
//!   manejador de consola mata los árboles de las pestañas y muestra un aviso
//!   nativo con la cuenta antes de que el sistema termine el proceso.
//!
//! Todo es best-effort: si no hay ventana de consola (`GetConsoleWindow`
//! devuelve NULL) o el cierre lo inicia el sistema, no se hace nada raro.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Solo actúa cuando el menú TUI lo instala (no en tests ni en tareas CLI).
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Pestañas de tarea abiertas.
static OPEN_TABS: AtomicUsize = AtomicUsize::new(0);
/// PIDs de los shells de las pestañas, para matar sus árboles al cerrar.
static PANE_PIDS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Instala el manejador de cierre de consola. Idempotente.
pub fn install() {
    #[cfg(windows)]
    {
        if !INSTALLED.swap(true, Ordering::Relaxed) {
            unsafe {
                win::SetConsoleCtrlHandler(Some(on_ctrl), 1);
            }
        }
    }
}

/// Sincroniza el número de pestañas internas abiertas.
///
/// Solo toca la ventana cuando se cruza de 0 a >0 (o al revés): llamar a
/// `EnableMenuItem` en cada fotograma sería desperdicio.
pub fn set_open_tabs(n: usize) {
    let prev = OPEN_TABS.swap(n, Ordering::Relaxed);
    if !INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    if (prev == 0) != (n == 0) {
        set_host_close_enabled(n == 0);
    }
}

/// Registra el PID del shell de una pestaña recién abierta.
pub fn register_pane(pid: u32) {
    if pid == 0 {
        return;
    }
    if let Ok(mut pids) = PANE_PIDS.lock() {
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
}

/// Olvida una pestaña cerrada por el usuario.
pub fn unregister_pane(pid: u32) {
    if let Ok(mut pids) = PANE_PIDS.lock() {
        pids.retain(|&p| p != pid);
    }
}

/// Texto del aviso nativo. Separado para poder verificarlo sin Win32.
fn warning_text(tabs: usize) -> String {
    let (sustantivo, plural) = if tabs == 1 {
        ("pestaña", "")
    } else {
        ("pestañas", "s")
    };
    format!(
        "Se está cerrando la ventana de Compass.\n\n\
         Hay {tabs} {sustantivo} de tarea abierta{plural} dentro de Compass.\n\
         Al cerrar esta ventana se terminarán sus procesos.\n\n\
         Para cerrar Compass de forma segura, usa Esc dentro de la app: \
         te avisa y puedes confirmar antes de cerrar las pestañas."
    )
}

#[cfg(windows)]
fn kill_pane_trees() {
    let Ok(pids) = PANE_PIDS.try_lock() else {
        return;
    };
    for pid in pids.iter().copied() {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(windows)]
fn set_host_close_enabled(enabled: bool) {
    unsafe {
        let hwnd = win::GetConsoleWindow();
        if hwnd.is_null() {
            return;
        }
        let menu = win::GetSystemMenu(hwnd, 0);
        if menu.is_null() {
            return;
        }
        let state = if enabled {
            win::MF_ENABLED
        } else {
            win::MF_GRAYED
        };
        win::EnableMenuItem(menu, win::SC_CLOSE, win::MF_BYCOMMAND | state);
        // El botón `×` solo refleja el cambio al repintar la barra de menús.
        win::DrawMenuBar(hwnd);
    }
}

#[cfg(not(windows))]
fn set_host_close_enabled(_enabled: bool) {}

#[cfg(windows)]
unsafe extern "system" fn on_ctrl(ctrl_type: u32) -> i32 {
    let closing = ctrl_type == win::CTRL_CLOSE_EVENT;
    let session_end = ctrl_type == win::CTRL_LOGOFF_EVENT || ctrl_type == win::CTRL_SHUTDOWN_EVENT;
    if !closing && !session_end {
        return 0;
    }
    let tabs = OPEN_TABS.load(Ordering::Relaxed);
    if tabs == 0 {
        return 0;
    }

    // El host va a terminar este proceso: sin esto, cargo/gradlew/flutter
    // quedarían vivos escribiendo en una ConPTY que ya no existe.
    kill_pane_trees();
    if closing {
        show_warning(tabs);
    }
    1
}

#[cfg(windows)]
fn show_warning(tabs: usize) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let title: Vec<u16> = OsStr::new("Compass - Pestañas abiertas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let msg: Vec<u16> = OsStr::new(&warning_text(tabs))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        win::MessageBoxW(
            std::ptr::null_mut(),
            msg.as_ptr(),
            title.as_ptr(),
            win::MB_OK | win::MB_ICONWARNING | win::MB_TOPMOST | win::MB_SETFOREGROUND,
        );
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    pub const CTRL_CLOSE_EVENT: u32 = 2;
    pub const CTRL_LOGOFF_EVENT: u32 = 5;
    pub const CTRL_SHUTDOWN_EVENT: u32 = 6;

    pub const SC_CLOSE: u32 = 0xF060;
    pub const MF_BYCOMMAND: u32 = 0x0000_0000;
    pub const MF_ENABLED: u32 = 0x0000_0000;
    pub const MF_GRAYED: u32 = 0x0000_0001;

    pub const MB_OK: u32 = 0x0000_0000;
    pub const MB_ICONWARNING: u32 = 0x0000_0030;
    pub const MB_TOPMOST: u32 = 0x0004_0000;
    pub const MB_SETFOREGROUND: u32 = 0x0001_0000;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
        pub fn GetConsoleWindow() -> *mut c_void;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn MessageBoxW(
            hwnd: *mut c_void,
            text: *const u16,
            caption: *const u16,
            type_: u32,
        ) -> i32;
        pub fn GetSystemMenu(hwnd: *mut c_void, revert: i32) -> *mut c_void;
        pub fn EnableMenuItem(menu: *mut c_void, id: u32, enable: u32) -> i32;
        pub fn DrawMenuBar(hwnd: *mut c_void) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_text_counts_tabs_and_points_to_esc() {
        let one = warning_text(1);
        assert!(one.contains("1 pestaña de tarea abierta "), "{one}");
        assert!(one.contains("usa Esc dentro de la app"), "{one}");

        let many = warning_text(3);
        assert!(many.contains("3 pestañas de tarea abiertas "), "{many}");
        assert!(many.contains("se terminarán sus procesos"), "{many}");
    }

    /// Sin instalar la guarda (como en los tests) el contador no toca Win32.
    #[test]
    fn set_open_tabs_is_inert_until_installed() {
        set_open_tabs(2);
        assert_eq!(OPEN_TABS.load(Ordering::Relaxed), 2);
        set_open_tabs(0);
        assert_eq!(OPEN_TABS.load(Ordering::Relaxed), 0);
    }
}
