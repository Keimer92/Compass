use std::io::{self, Write};
use anyhow::Result;
use crate::utils::{cmd, context::ProjectContext};

const PROFILE_CONTENT: &str = r#"# 1. Autocompletado visual del historial
Set-PSReadLineOption -PredictionSource History

# 2. TAB acepta la sugerencia COMPLETA
Set-PSReadLineKeyHandler -Key Tab -Function AcceptSuggestion

# 3. Flecha Derecha acepta palabra por palabra
Set-PSReadLineKeyHandler -Key RightArrow -Function AcceptNextSuggestionWord
"#;

pub fn configure(_ctx: &ProjectContext) -> Result<()> {
    // Esta tarea escribe un perfil de PowerShell (PSReadLine), que no existe en
    // Linux. La tarjeta está oculta en el menú, pero la task sigue alcanzable
    // por CLI, así que se informa en vez de escribir un archivo inútil.
    if !crate::utils::platform::IS_WINDOWS {
        println!();
        println!("[INFO] 'Configurar Terminal' sólo está disponible en Windows.");
        println!("       Escribe un perfil de PowerShell (PSReadLine), que no aplica en Linux.");
        println!();
        println!("Alternativa equivalente para bash/zsh:");
        println!("  - Instala el autocompletado: sudo apt install bash-completion");
        println!("  - O usa fzf: sudo apt install fzf && \"$(command -v fzf)\" --bash >> ~/.bashrc");
        println!();
        pause();
        return Ok(());
    }

    println!();
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║     CONFIGURAR TERMINAL - AUTOCOMPLETADO         ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("Esto configurará tu perfil de PowerShell con:");
    println!("  1. Autocompletado visual del historial (PredictionSource History)");
    println!("  2. TAB acepta la sugerencia COMPLETA");
    println!("  3. Flecha Derecha acepta palabra por palabra");
    println!();

    let profile_dir = dirs::document_dir()
        .unwrap_or_else(|| {
            crate::utils::platform::home_dir().unwrap_or_else(|| ".".into())
        })
        .join("WindowsPowerShell");

    let profile_file = profile_dir.join("Microsoft.PowerShell_profile.ps1");

    if !profile_dir.exists() {
        std::fs::create_dir_all(&profile_dir)?;
    }

    std::fs::write(&profile_file, PROFILE_CONTENT)?;

    println!("[LISTO] La configuración se ha guardado en:");
    println!("  {}", profile_file.display());
    println!();
    println!("Abre una nueva terminal para disfrutar del autocompletado.");
    println!();

    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
