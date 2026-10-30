//! Prueba de extremo a extremo del contenedor de terminal.
//!
//! Arranca el `compass.exe` real dentro de una ConPTY (como una ventana de
//! terminal cualquiera), lo maneja con teclas y comprueba que:
//!
//! 1. una card abre una **pestaña** dentro de Compass (barra `Inicio │ 1 Git`),
//! 2. la tarea corre dentro de esa pestaña (se ve su submenú),
//! 3. F12 vuelve al menú sin cerrar la pestaña,
//! 4. Esc avisa de que hay pestañas abiertas y Esc cancela la salida,
//! 5. F12 de nuevo regresa a la pestaña,
//! 6. Ctrl+W cierra la pestaña y deja el menú.
//!
//! Se ejecuta a mano porque depende del sistema y tarda unos segundos:
//!
//! ```text
//! cargo test --test container_e2e -- --ignored --nocapture
//! ```

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

struct Harness {
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    parser: Arc<Mutex<vt100::Parser>>,
}

impl Harness {
    fn contents(&self) -> String {
        let parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        parser.screen().contents()
    }

    fn wait_until(&self, what: &str, pred: impl Fn(&str) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(40);
        while Instant::now() < deadline {
            if pred(&self.contents()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        panic!(
            "tiempo agotado esperando {what}.\n--- pantalla ---\n{}\n--- fin ---",
            self.contents()
        );
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("escribir en la ConPTY");
        self.writer.flush().expect("vaciar la ConPTY");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
#[ignore = "abre la TUI real en una ConPTY y la maneja con teclas"]
fn card_opens_a_session_tab_and_f12_switches_between_menu_and_tab() {
    // Proyecto de prueba mínimo: un Cargo.toml basta para detectar Rust.
    let project = std::env::temp_dir().join(format!("compass-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&project).expect("crear proyecto temporal");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"e2e-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("Cargo.toml de prueba");

    let (rows, cols) = (35, 120);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("abrir ConPTY");

    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_compass"));
    command.cwd(&project);
    // El proceso raíz no debe creerse dentro de una pestaña.
    command.env_remove("COMPASS_EMBEDDED");
    command.env_remove("COMPASS_EXTERNAL_WINDOWS");
    command.env("RUST_BACKTRACE", "1");

    let child = pair.slave.spawn_command(command).expect("lanzar compass");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("lector");
    let writer = pair.master.take_writer().expect("escritor");
    let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 2000)));
    {
        let parser = Arc::clone(&parser);
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut parser) = parser.lock() {
                    parser.process(&buf[..n]);
                }
            }
        });
    }

    let mut h = Harness {
        writer,
        child,
        parser,
    };

    // 1. Menú principal visible.
    h.wait_until("el menú principal", |s| s.contains("Proyecto:"));

    // 2. Pestaña CONFIGURACIÓN y card Git (primera de esa pestaña).
    h.send(b"\t");
    h.wait_until("la pestaña CONFIGURACIÓN", |s| {
        s.contains("CONFIGURACION")
    });

    // 3. Enter: la card abre una pestaña y corre `compass --task git` dentro.
    h.send(b"\r");
    h.wait_until("la barra de pestañas", |s| {
        // La primera fila es la barra: pestaña del menú llamada Compass y
        // pestaña de tarea con su ×, cada una en una caja redondeada.
        s.lines().next().is_some_and(|bar| {
            bar.contains("Compass")
                && bar.contains("1 Git")
                && bar.contains('\u{d7}')
                && bar.contains('\u{256d}')
                && bar.contains('\u{256e}')
        })
    });
    // El submenú reutiliza el estilo del menú principal: cabecera con
    // "Proyecto:", fila de sección "GIT 5" y pie con "volver".
    h.wait_until("el submenú de Git dentro de la pestaña", |s| {
        s.contains("Inicializar reposit") && s.contains("GIT 5") && s.contains("volver")
    });

    // 4. F12 vuelve al menú sin cerrar la pestaña. La espera no puede fiarse
    //    de "Proyecto:": el submenú reutiliza la cabecera y también lo pinta.
    //    La fila de pestañas solo existe en el menú.
    h.send(b"\x1b[24~");
    h.wait_until("el menú tras F12", |s| {
        s.contains("DESARROLLO · MNT") && s.contains("1 Git")
    });

    // 5. Esc con la pestaña abierta no sale: avisa de que hay pestañas
    //    internas y sus procesos se cerrarían.
    h.send(b"\x1b");
    h.wait_until("el aviso de pestañas abiertas", |s| {
        s.contains("pestaña abierta")
            && s.contains("proceso en ejecución")
            && s.contains("cancelar")
    });

    // 6. El segundo Esc cancela el aviso y deja el menú intacto.
    h.send(b"\x1b");
    h.wait_until("el menú tras cancelar la salida", |s| {
        s.contains("DESARROLLO · MNT") && !s.contains("pestaña abierta")
    });

    // 7. F12 regresa a la pestaña (su pantalla seguía viva).
    h.send(b"\x1b[24~");
    h.wait_until("la pestaña tras el segundo F12", |s| {
        s.contains("Inicializar reposit") && s.contains("1 Git")
    });

    // 8. Ctrl+W cierra la pestaña y vuelve al menú.
    h.send(b"\x17");
    h.wait_until("el cierre de la pestaña", |s| {
        s.contains("Proyecto:") && !s.contains("1 Git")
    });

    // 9. Esc sale de Compass (ya sin pestañas, sin aviso); el proceso hijo
    //    debe terminar.
    //
    // El cierre con el botón `×` del ratón no se prueba aquí: crossterm lee
    // los eventos de ratón de los registros de consola de Windows, y una
    // ConPTY no los genera a partir de las secuencias VT que escribe el padre.
    h.send(b"\x1b");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Some(_)) = h.child.try_wait() {
            break;
        }
        if Instant::now() > deadline {
            panic!("compass no terminó tras Esc");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = std::fs::remove_dir_all(&project);
}
