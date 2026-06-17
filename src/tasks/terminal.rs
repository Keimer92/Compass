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
        .unwrap_or_else(|| std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()))
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
