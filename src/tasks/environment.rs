//! Respaldo y restauración del entorno de desarrollo de Windows.
//!
//! Migrado del proyecto **Variables Backupper**: guarda variables de entorno
//! (Sistema y Usuario), el inventario de herramientas (Java, Flutter, Android,
//! Rust, Kotlin, Gradle, .NET, Node, Python, Visual Studio, VS Code, Git,
//! Chocolatey, Winget) y copia los archivos de configuración que no se pueden
//! regenerar solos (`.gitconfig`, claves SSH, ajustes de VS Code, keystores…).
//!
//! Los dos scripts de PowerShell se **embeben** en el binario y se escriben en
//! `<data_dir>/compass/env-backup/` antes de ejecutarlos. Así `compass.exe`
//! sigue siendo un único archivo (lo que copia `install.bat`) y el respaldo no
//! depende de que exista el repositorio original junto al ejecutable.
//!
//! El flujo es un submenú con el mismo aspecto que el menú principal (ver
//! [`crate::tasks::submenu`]). Las acciones corren el script en la consola de la
//! pestaña, con stdio heredado: los `Read-Host` del script (confirmaciones,
//! elección de backup) siguen funcionando tal cual.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::menu;
use crate::tasks::submenu::{Submenu, SubmenuAction, SubmenuItem};
use crate::theme;
use crate::utils::{cmd, context::ProjectContext, platform, ui};

/// Scripts embebidos. El contenido se escribe tal cual (el de restauración
/// lleva BOM UTF-8 porque PowerShell 5.1 lo necesita para leer los acentos).
const BACKUP_SCRIPT: &str = include_str!("../../assets/env-backup/Backup-Variables.ps1");
const RESTORE_SCRIPT: &str = include_str!("../../assets/env-backup/Restore-Variables.ps1");
const BACKUP_FILE: &str = "Backup-Variables.ps1";
const RESTORE_FILE: &str = "Restore-Variables.ps1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvAction {
    Backup,
    Inventory,
    Restore,
    Validate,
    OpenFolder,
    Back,
}

/// Carpeta de trabajo: scripts y respaldos por defecto.
fn work_dir() -> Result<PathBuf> {
    let dir = platform::data_dir()
        .context("No se pudo determinar LOCALAPPDATA para guardar el entorno")?
        .join("compass")
        .join("env-backup");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("No se pudo crear {}", dir.display()))?;
    Ok(dir)
}

/// Escribe los scripts embebidos sólo si faltan o cambiaron.
fn materialize(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, content) in [(BACKUP_FILE, BACKUP_SCRIPT), (RESTORE_FILE, RESTORE_SCRIPT)] {
        let path = dir.join(name);
        let stale = std::fs::read_to_string(&path)
            .map(|current| current != content)
            .unwrap_or(true);
        if stale {
            std::fs::write(&path, content)
                .with_context(|| format!("No se pudo escribir {}", path.display()))?;
        }
    }
    Ok(())
}

fn env_entries() -> Vec<(EnvAction, SubmenuItem)> {
    vec![
        (
            EnvAction::Backup,
            SubmenuItem {
                icon: "\u{1f4be}",
                label: "Backup completo",
                description: "Variables, inventario y archivos",
                color_key: "environment",
            },
        ),
        (
            EnvAction::Inventory,
            SubmenuItem {
                icon: "\u{1f4cb}",
                label: "Solo inventario",
                description: "Herramientas y versiones, sin copiar",
                color_key: "version",
            },
        ),
        (
            EnvAction::Restore,
            SubmenuItem {
                icon: "\u{267b}",
                label: "Restaurar entorno",
                description: "Variables, rutas y rollback",
                color_key: "clean",
            },
        ),
        (
            EnvAction::Validate,
            SubmenuItem {
                icon: "\u{1f50d}",
                label: "Validar backup",
                description: "Diff sin modificar nada",
                color_key: "debug",
            },
        ),
        (
            EnvAction::OpenFolder,
            SubmenuItem {
                icon: "\u{1f4c2}",
                label: "Abrir respaldos",
                description: "Carpeta de respaldos de Compass",
                color_key: "git",
            },
        ),
        (
            EnvAction::Back,
            SubmenuItem {
                icon: "\u{21a9}",
                label: "Volver al menu principal",
                description: "Regresa al menu principal",
                color_key: "terminal",
            },
        ),
    ]
}

/// Submenú de entorno: backup, inventario, restauración y validación.
pub fn manage(ctx: &ProjectContext) -> Result<()> {
    let work = work_dir()?;
    if let Err(e) = materialize(&work) {
        cmd::clear_screen();
        eprintln!("[ERROR] No se pudieron preparar los scripts de respaldo: {e:#}");
        ui::pause();
        return Ok(());
    }

    let theme = theme::load();
    let mut menu = Submenu::new("ENTORNO · RESPALDO", "CFG", env_entries());

    let mut terminal = menu::setup_terminal()?;
    cmd::set_console_title(&ctx.window_title());

    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| menu.draw(f, f.area(), ctx, theme))?;
            match menu.handle_event()? {
                SubmenuAction::Back => return Ok(()),
                SubmenuAction::Continue => {}
                SubmenuAction::Run(item) => {
                    if item == EnvAction::Back {
                        return Ok(());
                    }
                    // La acción usa println / Read-Host: se devuelve la
                    // consola normal y se restaura el TUI al terminar.
                    menu::restore_terminal(&mut terminal)?;
                    let action_result = run_action(item, &work);
                    terminal = menu::setup_terminal()?;
                    cmd::set_console_title(&ctx.window_title());
                    action_result?;
                }
            }
        }
    })();

    let _ = menu::restore_terminal(&mut terminal);
    result
}

fn run_action(action: EnvAction, work: &Path) -> Result<()> {
    match action {
        EnvAction::Backup => run_backup(work, false),
        EnvAction::Inventory => run_backup(work, true),
        EnvAction::Restore => run_restore(work, false),
        EnvAction::Validate => run_restore(work, true),
        EnvAction::OpenFolder => open_backups(work),
        EnvAction::Back => Ok(()),
    }
}

fn default_backup_root(work: &Path) -> PathBuf {
    work.join("Backup")
}

fn backup_script(work: &Path) -> PathBuf {
    work.join(BACKUP_FILE)
}

fn restore_script(work: &Path) -> PathBuf {
    work.join(RESTORE_FILE)
}

/// Respaldo completo o solo inventario. Pregunta destino y cuántos respaldos
/// conservar; el resto de confirmaciones (unidad `C:`, borrado de antiguos) las
/// sigue haciendo el propio script.
fn run_backup(work: &Path, inventory_only: bool) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner(if inventory_only {
        "INVENTARIO DEL ENTORNO"
    } else {
        "BACKUP DEL ENTORNO DE DESARROLLO"
    });
    println!();
    if inventory_only {
        println!("  Guarda herramientas y versiones sin copiar archivos.");
    } else {
        println!("  Variables, inventario y archivos de configuracion");
        println!("  (.gitconfig, SSH, VS Code, keystores...).");
    }
    println!("  Ejecuta Compass como Administrador para incluir las variables");
    println!("  de SISTEMA.");
    println!();

    let default_dest = default_backup_root(work);
    let dest = ui::ask_text(
        "Destino de los respaldos",
        Some(&default_dest.display().to_string()),
    )?;
    let dest = if dest.trim().is_empty() {
        default_dest.display().to_string()
    } else {
        dest
    };

    let keep = ui::ask_text("Respaldos a conservar", Some("1"))?;
    let keep: u32 = keep.trim().parse().unwrap_or(1).clamp(1, 99);

    let mut script = backup_script(work);
    if !script.exists() {
        materialize(work)?;
        script = backup_script(work);
    }

    let mut args = vec!["-Destino".to_string(), dest];
    if inventory_only {
        args.push("-SoloInventario".to_string());
    }
    args.push("-ConservarN".to_string());
    args.push(keep.to_string());

    println!();
    let ok = cmd::run_ps_file_status(&script, &args, work, &[])?;
    if !ok {
        println!();
        println!("[AVISO] El respaldo termino con fallos; revisa el resumen de arriba.");
    }
    println!();
    ui::pause();
    Ok(())
}

/// Restauración o validación (`-SoloValidar`). Si hay respaldos en la carpeta de
/// Compass, propone el más reciente; con el campo vacío el script lista los que
/// encuentre o pide una ruta.
fn run_restore(work: &Path, validate_only: bool) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner(if validate_only {
        "VALIDAR BACKUP DEL ENTORNO"
    } else {
        "RESTAURAR ENTORNO DE DESARROLLO"
    });
    println!();
    if validate_only {
        println!("  Solo informa: no modifica variables ni archivos.");
    } else {
        println!("  Restaura variables, adapta rutas de usuario y guarda un");
        println!("  respaldo del estado actual antes de tocar nada.");
        println!("  Ejecuta Compass como Administrador para restaurar las");
        println!("  variables de SISTEMA.");
    }
    println!();

    let latest = latest_backup(work);
    let default = latest.as_ref().map(|p| p.display().to_string());
    let folder = ui::ask_text("Carpeta del backup", default.as_deref())?;

    let mut script = restore_script(work);
    if !script.exists() {
        materialize(work)?;
        script = restore_script(work);
    }

    let mut args = Vec::new();
    if !folder.trim().is_empty() {
        args.push("-CarpetaBackup".to_string());
        args.push(folder);
    }
    if validate_only {
        args.push("-SoloValidar".to_string());
    } else {
        let files = ui::ask_text(
            "Restaurar tambien archivos (git/ssh/VS Code)? (s/N)",
            Some("n"),
        )?;
        if is_yes(&files) {
            args.push("-RestaurarArchivos".to_string());
        }
    }

    println!();
    let ok = cmd::run_ps_file_status(&script, &args, work, &[])?;
    if !ok {
        println!();
        println!("[AVISO] La restauracion termino con errores; revisa el resumen de arriba.");
    }
    println!();
    ui::pause();
    Ok(())
}

fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    )
}

/// Respaldo más reciente (`EnvBackup_*`) de la carpeta de Compass. Los nombres
/// llevan `AAAA-MM-DD_HHMMSS`, así que el orden alfabético es el cronológico.
fn latest_backup(work: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(default_backup_root(work))
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .map(|name| name.to_string_lossy().starts_with("EnvBackup_"))
                    .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    dirs.pop()
}

fn open_backups(work: &Path) -> Result<()> {
    let root = default_backup_root(work);
    std::fs::create_dir_all(&root)?;
    Command::new("explorer.exe")
        .arg(&root)
        .spawn()
        .with_context(|| format!("No se pudo abrir {}", root.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "compass-env-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn embedded_scripts_are_the_real_ones() {
        assert!(BACKUP_SCRIPT.contains("function Invoke-Native"));
        assert!(BACKUP_SCRIPT.contains("-Destino"));
        assert!(RESTORE_SCRIPT.contains("-CarpetaBackup"));
        assert!(RESTORE_SCRIPT.contains("pre-restore_"));
        assert!(BACKUP_SCRIPT.len() > 50_000, "backup incompleto");
        assert!(RESTORE_SCRIPT.len() > 30_000, "restore incompleto");
    }

    #[test]
    fn materialize_writes_both_scripts_and_is_idempotent() {
        let dir = temp_dir("materialize");
        materialize(&dir).unwrap();
        let backup = dir.join(BACKUP_FILE);
        let restore = dir.join(RESTORE_FILE);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), BACKUP_SCRIPT);
        assert_eq!(std::fs::read_to_string(&restore).unwrap(), RESTORE_SCRIPT);

        // Un archivo manipulado se reescribe; uno correcto se deja intacto.
        std::fs::write(&backup, "roto").unwrap();
        materialize(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), BACKUP_SCRIPT);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_backup_uses_the_newest_folder_name() {
        let dir = temp_dir("latest");
        let root = default_backup_root(&dir);
        for name in ["EnvBackup_2020-01-01_000000", "EnvBackup_2026-10-06_220453", "otra"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        let latest = latest_backup(&dir).expect("debe encontrar respaldos");
        assert_eq!(
            latest.file_name().unwrap().to_string_lossy(),
            "EnvBackup_2026-10-06_220453"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn entries_cover_each_action_once_and_end_with_back() {
        let entries = env_entries();
        let actions: Vec<EnvAction> = entries.iter().map(|(action, _)| *action).collect();
        assert_eq!(actions.len(), 6);
        for action in [
            EnvAction::Backup,
            EnvAction::Inventory,
            EnvAction::Restore,
            EnvAction::Validate,
            EnvAction::OpenFolder,
        ] {
            assert_eq!(
                actions.iter().filter(|a| **a == action).count(),
                1,
                "acción repetida o ausente: {action:?}"
            );
        }
        assert_eq!(*actions.last().unwrap(), EnvAction::Back);

        let mut keys = Vec::new();
        for (_, item) in &entries {
            assert!(
                crate::theme::CARD_KEYS.contains(&item.color_key),
                "clave {} fuera de CARD_KEYS",
                item.color_key
            );
            keys.push(item.color_key);
        }
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                assert_ne!(keys[i], keys[j], "clave de color repetida: {}", keys[i]);
            }
        }
    }

    #[test]
    fn yes_answers_are_accepted() {
        for answer in ["s", "S", "si", "sí", "y", "yes", " Si "] {
            assert!(is_yes(answer), "{answer:?} debería contar como sí");
        }
        for answer in ["", "n", "no", "nel"] {
            assert!(!is_yes(answer), "{answer:?} no debería contar como sí");
        }
    }
}
