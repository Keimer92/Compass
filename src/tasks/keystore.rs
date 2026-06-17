use std::io::{self, Write};
use anyhow::Result;
use chrono::Local;
use inquire::Select;

use crate::utils::{cmd, context::ProjectContext};
use crate::tasks::signer;

pub fn manage(ctx: &ProjectContext) -> Result<()> {
    loop {
        cmd::clear_screen();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║           GESTIÓN DE LLAVES (KEYSTORE)          ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();

        let options = vec![
            "1. Abrir herramienta de firmas (crear/usar/importar)",
            "2. Respaldar keystore actual",
            "3. Restaurar keystore desde respaldo",
            "4. Ver respaldos disponibles",
            "5. Volver al menú principal",
        ];

        let choice = Select::new("Selecciona una opción:", options)
            .with_vim_mode(true)
            .prompt()?;

        cmd::clear_screen();

        match choice {
            "1. Abrir herramienta de firmas (crear/usar/importar)" => {
                signer::create_or_import(ctx)?;
            }
            "2. Respaldar keystore actual" => {
                backup_keystore(ctx)?;
            }
            "3. Restaurar keystore desde respaldo" => {
                restore_keystore(ctx)?;
            }
            "4. Ver respaldos disponibles" => {
                list_backups(ctx)?;
            }
            _ => break,
        }
    }
    Ok(())
}

fn backup_keystore(ctx: &ProjectContext) -> Result<()> {
    println!();
    println!("--- RESPALDAR KEYSTORE ---");

    let android_app = ctx.script_dir.join("src-tauri\\gen\\android\\app");
    if !android_app.exists() {
        eprintln!("[ERROR] No existe el directorio Android del proyecto.");
        pause();
        return Ok(());
    }

    let keystore_files: Vec<_> = std::fs::read_dir(&android_app)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.ends_with(".keystore") || name.ends_with(".jks")
        })
        .collect();

    if keystore_files.is_empty() {
        eprintln!();
        eprintln!("[ERROR] No se encontraron archivos .keystore o .jks en el proyecto.");
        eprintln!("La ruta buscada es: {}", android_app.display());
        eprintln!("Primero crea uno con la opción 1 (Abrir herramienta de firmas).");
        pause();
        return Ok(());
    }

    let backup_dir = ctx.script_dir.join("keystore-backups");
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let dest_dir = backup_dir.join(&timestamp);
    std::fs::create_dir_all(&dest_dir)?;

    for entry in &keystore_files {
        let name = entry.file_name();
        std::fs::copy(entry.path(), dest_dir.join(&name))?;
        println!("  [OK] {} respaldado.", name.to_string_lossy());
    }

    let props = android_app.join("keystore.properties");
    if props.exists() {
        std::fs::copy(&props, dest_dir.join("keystore.properties"))?;
        println!("  [OK] keystore.properties respaldado.");
    }

    println!();
    println!("[OK] Respaldo completado en:");
    println!("     {}", dest_dir.display());
    println!();
    println!("IMPORTANTE: Guarda esta carpeta en un lugar seguro (USB, nube).");
    pause();
    Ok(())
}

fn restore_keystore(ctx: &ProjectContext) -> Result<()> {
    println!();
    println!("--- RESTAURAR KEYSTORE ---");

    let backup_dir = ctx.script_dir.join("keystore-backups");
    if !backup_dir.exists() {
        eprintln!("[ERROR] No existe la carpeta de respaldos.");
        eprintln!("Primero debes crear un respaldo con la opción 2.");
        pause();
        return Ok(());
    }

    let backups: Vec<_> = std::fs::read_dir(&backup_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();

    if backups.is_empty() {
        eprintln!("[ERROR] No hay respaldos disponibles.");
        pause();
        return Ok(());
    }

    let backup_names: Vec<String> = backups.iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let count = std::fs::read_dir(p).map(|d| d.count()).unwrap_or(0);
            format!("{} ({} archivo(s))", name, count)
        })
        .collect();

    let selection = Select::new("Selecciona respaldo a restaurar:", backup_names)
        .prompt()?;

    let idx = selection.find(" (").unwrap_or(selection.len());
    let selected_name = &selection[..idx];
    let selected_path = backup_dir.join(selected_name);

    if !selected_path.exists() {
        eprintln!("[ERROR] El respaldo seleccionado ya no existe.");
        pause();
        return Ok(());
    }

    let android_app = ctx.script_dir.join("src-tauri\\gen\\android\\app");
    let mut count = 0;
    for entry in std::fs::read_dir(&selected_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name();
            std::fs::copy(&path, android_app.join(&name))?;
            println!("  [OK] {} restaurado.", name.to_string_lossy());
            count += 1;
        }
    }

    println!();
    println!("[OK] Restauración completada ({} archivo(s)).", count);
    println!("Ya puedes compilar tu Android en modo release con firma.");
    pause();
    Ok(())
}

fn list_backups(ctx: &ProjectContext) -> Result<()> {
    println!();
    println!("--- RESPALDOS DISPONIBLES ---");

    let backup_dir = ctx.script_dir.join("keystore-backups");
    if !backup_dir.exists() {
        println!("No existe la carpeta de respaldos.");
        println!("Usa la opción 2 para crear tu primer respaldo.");
        pause();
        return Ok(());
    }

    let mut backups: Vec<_> = std::fs::read_dir(&backup_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    if backups.is_empty() {
        println!("No hay respaldos aún.");
        println!("Usa la opción 2 para crear tu primer respaldo.");
    } else {
        backups.sort_by_key(|b| b.path().file_name().unwrap().to_os_string());
        println!();
        println!("Total de respaldos: {}", backups.len());
        println!();
        for entry in &backups {
            let name = entry.file_name().to_string_lossy().to_string();
            let files: Vec<_> = std::fs::read_dir(entry.path())
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .collect();
            let total_size: u64 = files.iter()
                .filter_map(|f| f.metadata().ok())
                .map(|m| m.len())
                .sum();
            println!("  {}  |  {} archivo(s)  |  {:.2} KB", name, files.len(), total_size as f64 / 1024.0);
        }
    }
    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
