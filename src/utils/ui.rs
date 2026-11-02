use anyhow::Result;
use std::io::{self, IsTerminal, Write};

pub fn ask_text(prompt: &str, default: Option<&str>) -> Result<String> {
    let msg = match default {
        Some(d) => format!("{} [{}]: ", prompt, d),
        None => format!("{}: ", prompt),
    };
    print!("{}", msg);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(trimmed)
    }
}

pub fn ask_password(prompt: &str) -> Result<String> {
    print!("{}: ", prompt);
    io::stdout().flush()?;

    // Sin una terminal real (stdin redirigido, p. ej. desde un script) no se
    // puede activar el modo raw: se lee una línea normal. Antes se intentaba y
    // `read()` se quedaba bloqueado esperando teclas que nunca llegan.
    if !io::stdin().is_terminal() {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        println!();
        return Ok(input.trim().to_string());
    }

    use crossterm::event::{read, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

    let mut password = String::new();

    if enable_raw_mode().is_ok() {
        loop {
            if let Ok(Event::Key(key)) = read() {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Enter => {
                            println!();
                            break;
                        }
                        KeyCode::Char(c) => {
                            password.push(c);
                            let _ = print!("*");
                            let _ = io::stdout().flush();
                        }
                        KeyCode::Backspace => {
                            if password.pop().is_some() {
                                let _ = print!("\x08 \x08");
                                let _ = io::stdout().flush();
                            }
                        }
                        KeyCode::Esc => {
                            println!();
                            password.clear();
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
        let _ = disable_raw_mode();
    } else {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        password = input.trim().to_string();
    }

    Ok(password)
}

/// Ancho en columnas de un texto, contando cada `char` como una.
///
/// `str::len()` devuelve bytes, y los emojis de los banners (✅, ℹ️) ocupan 3-4
/// bytes pero 1-2 columnas. Centrar con `len()` descentraba la línea varios
/// caracteres; por eso el padding se calcula sobre el conteo de caracteres.
fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Imprime un banner con bordes de caja y el texto centrado.
fn banner(icon: &str, title: &str) {
    let inner = format!("  {icon}  {title}  ");
    let inner_w = display_width(&inner);
    let width = inner_w.max(48);
    let h_border = "\u{2500}".repeat(width);
    // Relleno para centrar: `width - inner_w`, partido por los dos lados.
    let left = (width - inner_w) / 2;
    let right = width - inner_w - left;

    println!();
    println!("  \u{250C}{}\u{2510}", h_border);
    println!(
        "  \u{2502}{}{}{}\u{2502}",
        " ".repeat(left),
        inner,
        " ".repeat(right)
    );
    println!("  \u{2514}{}\u{2518}", h_border);
    println!();
}

pub fn success_banner(title: &str) {
    banner("\u{2705}", title);
}

pub fn info_banner(title: &str) {
    banner("\u{2139}\u{FE0E}", title);
}

/// Banner de caja `╔═╗` con el título centrado.
///
/// El centrado cuenta **caracteres**, no bytes: así los acentos (á) y los emoji
/// no desajustan el borde derecho.
pub fn box_banner(title: &str) {
    let width = title.chars().count().max(48);
    let pad = width - title.chars().count();
    let left = pad / 2;
    let right = pad - left;
    println!("\u{2554}{}\u{2557}", "\u{2550}".repeat(width));
    println!(
        "\u{2551}{}{}{}\u{2551}",
        " ".repeat(left),
        title,
        " ".repeat(right)
    );
    println!("\u{255A}{}\u{255D}", "\u{2550}".repeat(width));
}

pub fn pause() {
    print!("  Presiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
}

/// Como [`pause`], pero en vez de esperar a Enter cuenta regresiva y **cierra
/// la pestaña** al llegar a cero.
///
/// Se usa en las tareas de compilación: el build ya terminó bien, no hay nada
/// que leer, así que la ventana se cierra sola pasados `seconds` segundos (en
/// vez de quedarse colgada esperando un Enter que nadie va a pulsar).
///
/// Dentro del contenedor no hay ventana que cerrar: lo que se cierra es la
/// pestaña, y eso lo decide el proceso padre mirando el código de salida. Por
/// eso la cuenta deja la marca `cmd::request_close_tab`, que `main` convierte
/// en `cmd::EXIT_CLOSE_TAB` **cuando la tarea termina**. Las dos llamadas que
/// caen en medio de un flujo (`cmd::run_build_and_reveal_*` y el `finish` del
/// submenú de release) siguen con su bucle: ahí la pestaña se cierra al salir
/// del submenú o al terminar la instalación, que es cuando de verdad se acabó.
pub fn pause_for(seconds: u64) {
    for remaining in (1..=seconds).rev() {
        print!("\r  Cerrando en {} s...  ", remaining);
        io::stdout().flush().ok();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    println!();
    if crate::utils::cmd::embedded() {
        crate::utils::cmd::request_close_tab();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La cuenta atrás deja la marca que hace salir al proceso con
    /// `cmd::EXIT_CLOSE_TAB`.
    ///
    /// Se prueba con `pause_for(0)`, cuyo rango `(1..=0)` está vacío: no duerme
    /// un segundo por cada cuenta y deja el flag en el mismo punto. La otra
    /// mitad del `if` (fuente del contenedor, donde la ventana ya se cierra
    /// sola al terminar el proceso) no se comprueba aquí porque el flag no se
    /// puede volver a bajar: sería una prueba dependiente del orden.
    #[test]
    fn the_countdown_asks_the_container_to_close_the_tab() {
        // `COMPASS_EMBEDDED` define el proceso entero, así que se limpia al
        // terminar para no contaminar otras pruebas.
        std::env::set_var("COMPASS_EMBEDDED", "1");
        pause_for(0);
        std::env::remove_var("COMPASS_EMBEDDED");

        assert!(crate::utils::cmd::close_tab_requested());
    }
}
