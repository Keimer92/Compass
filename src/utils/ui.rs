use std::io::{self, Write};
use anyhow::Result;

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

pub fn success_banner(title: &str) {
    let inner = format!("  \u{2705}  {}  \u{2705}  ", title);
    let width = inner.len().max(48);
    let h_border = "\u{2500}".repeat(width);
    println!();
    println!("  \u{250C}{}\u{2510}", h_border);
    println!("  \u{2502}{:^width$}\u{2502}", inner);
    println!("  \u{2514}{}\u{2518}", h_border);
    println!();
}

pub fn info_banner(title: &str) {
    let inner = format!("  \u{2139}  {}  ", title);
    let width = inner.len().max(48);
    let h_border = "\u{2500}".repeat(width);
    println!();
    println!("  \u{250C}{}\u{2510}", h_border);
    println!("  \u{2502}{:^width$}\u{2502}", inner);
    println!("  \u{2514}{}\u{2518}", h_border);
    println!();
}

pub fn pause() {
    print!("  Presiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
}
