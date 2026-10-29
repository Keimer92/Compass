use anyhow::Result;
use std::io::{self, Write};

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

/// Como [`pause`], pero en vez de esperar a Enter cuenta regresiva y cierra
/// automáticamente.
///
/// Se usa en las tareas de **compilación en modo debug** de Go + Wails v3:
/// el build ya terminó bien, no hay nada que leer, así que la ventana se cierra
/// sola pasados `seconds` segundos (en vez de quedarse colgada esperando un
/// Enter que nadie va a pulsar).
pub fn pause_for(seconds: u64) {
    for remaining in (1..=seconds).rev() {
        print!("\r  Cerrando en {} s...  ", remaining);
        io::stdout().flush().ok();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    println!();
}
