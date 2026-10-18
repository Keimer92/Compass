mod menu;
mod tasks;
mod theme;
mod utils;

use std::io::{self, Write};
use clap::Parser;
use anyhow::Result;

#[derive(Parser)]
#[command(
    name = "compass",
    about = "Compass - Multiplataforma (Tauri/Flutter/KMP)",
    version = "0.1.0",
)]
struct Cli {
    #[arg(short, long, help = "Ejecutar tarea directamente (ej: windows-dev, clean)")]
    task: Option<String>,

    #[arg(short, long, help = "Ruta del proyecto (se autodetecta por defecto)")]
    path: Option<String>,

    /// Elegir la carpeta del proyecto con un navegador, en vez de autodetectar.
    ///
    /// Necesario cuando se lanza desde el launcher de COSMIC, donde el
    /// directorio actual es `$HOME` y no hay ningún proyecto que detectar.
    #[arg(long, help = "Elegir la carpeta del proyecto con un navegador")]
    pick: bool,

    #[arg(long, help = "Registrar eventos del menú TUI en <tmpdir>/compass-tui.log")]
    trace: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(task) = &cli.task {
        if task.starts_with("context-menu")
            || task.starts_with("opencode")
            || task.starts_with("cline")
            || task.starts_with("kilo")
            || task.starts_with("antigravity")
            || task.starts_with("vibe")
        {
            return run_global_task(task);
        }
    }

    // ── Resolución de la carpeta del proyecto ──
    //
    // Tres caminos, en orden de prioridad:
    //   1. `--pick`      → abre el navegador siempre.
    //   2. `--path`      → usa la ruta explícita.
    //   3. autodetección → sube desde el CWD; si falla y no hay `--task`,
    //                     cae al navegador (así se puede usar desde el launcher,
    //                     que arranca en `$HOME` sin ningún proyecto).
    let dir = if cli.pick {
        match utils::context::pick_project_root(&std::env::current_dir()?)? {
            Some(d) => d,
            None => {
                println!("[INFO] No se eligió ningún proyecto.");
                return Ok(());
            }
        }
    } else if let Some(p) = &cli.path {
        let dir = std::path::PathBuf::from(p);
        if !dir.exists() {
            anyhow::bail!("La ruta especificada no existe: {}", p);
        }
        dir
    } else {
        std::env::current_dir()?
    };

    std::env::set_current_dir(&dir)?;

    let ctx = match utils::context::detect() {
        Ok(c) => c,
        Err(e) => {
            // Con `--task` el error es el resultado real: no tiene sentido
            // preguntar nada. Sin `--task` damos la opción de elegir carpeta.
            if cli.task.is_some() {
                return Err(e);
            }
            eprintln!("\n[WARN] {}\n", e);
            match utils::context::pick_project_root(&dir)? {
                Some(d) => {
                    std::env::set_current_dir(&d)?;
                    utils::context::detect()?
                }
                None => {
                    println!("[INFO] No se eligió ningún proyecto.");
                    return Ok(());
                }
            }
        }
    };

    if let Some(task) = &cli.task {
        run_task_direct(&ctx, task)?;
    } else {
        menu::set_trace(cli.trace);
        menu::run(&ctx)?;
    }

    Ok(())
}

fn run_global_task(task: &str) -> Result<()> {
    match task {
        "context-menu" => tasks::context_menu::toggle(),
        "context-menu-install" => tasks::context_menu::install(),
        "context-menu-uninstall" => tasks::context_menu::uninstall(),
        "opencode" => tasks::context_menu::toggle(),
        "opencode-install" => tasks::context_menu::install_opencode(),
        "opencode-uninstall" => tasks::context_menu::uninstall_opencode(),
        "cline-install" => tasks::context_menu::install_cline(),
        "cline-uninstall" => tasks::context_menu::uninstall_cline(),
        "kilo-install" => tasks::context_menu::install_kilocode(),
        "kilo-uninstall" => tasks::context_menu::uninstall_kilocode(),
        "antigravity-install" => tasks::context_menu::install_antigravity(),
        "antigravity-uninstall" => tasks::context_menu::uninstall_antigravity(),
        "vibe-setup" => tasks::context_menu::setup(),
        _ => anyhow::bail!("Tarea desconocida: {}", task),
    }
}

fn run_task_direct(ctx: &crate::utils::context::ProjectContext, task: &str) -> Result<()> {
    // Cmd muestra en el título el command line del hijo mientras corre; al
    // fijarlo aquí con el nombre del directorio del proyecto, la ventana de la
    // tarea muestra "School Tracker" en vez de la ruta del binario.
    crate::utils::cmd::set_console_title(&crate::utils::platform::dir_display_name(
        &ctx.script_dir,
    ));
    match task {
        "windows-dev" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::windows::dev(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::run_windows(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::run_windows(ctx),
            crate::utils::context::ProjectType::Rust => tasks::rust::run(ctx),
        },
        "android-dev" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::dev(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::run_android(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::run_android(ctx),
            crate::utils::context::ProjectType::Rust => anyhow::bail!("No aplicable a proyectos Rust"),
        },
        "windows-build" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::windows::build(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_windows(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_windows(ctx),
            crate::utils::context::ProjectType::Rust => tasks::rust::build(ctx),
        },
        "android-arm64" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_arm64(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_apk(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_apk(ctx),
            crate::utils::context::ProjectType::Rust => anyhow::bail!("No aplicable a proyectos Rust"),
        },
        "android-armv7" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_armv7(ctx),
            crate::utils::context::ProjectType::Flutter => anyhow::bail!("No aplicable a proyectos Flutter"),
            crate::utils::context::ProjectType::Kotlin => anyhow::bail!("No aplicable a proyectos Kotlin"),
            crate::utils::context::ProjectType::Rust => anyhow::bail!("No aplicable a proyectos Rust"),
        },
        "android-all" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_all(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_aab(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_aab(ctx),
            crate::utils::context::ProjectType::Rust => anyhow::bail!("No aplicable a proyectos Rust"),
        },
        "android-install-debug" => match ctx.project_type {
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::install_debug(ctx),
            _ => anyhow::bail!("No aplicable a proyectos Kotlin (solo Gradle)"),
        },
        // Limpieza: una sola ruta para el menú unificado de `cleanup` (rápida,
        // completa y reporte de tamaños). Los nombres antiguos siguen valiendo
        // como alias.
        "clean" | "clean-cache" | "clean-project" => tasks::cleanup::manage(ctx),
        "debug-android" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::debug(ctx),
            crate::utils::context::ProjectType::Flutter => {
                println!("[INFO] Para depurar Flutter: flutter run --debug");
                Ok(())
            }
            crate::utils::context::ProjectType::Kotlin => {
                println!("[INFO] Para depurar KMP: ./gradlew :composeApp:installDebug");
                Ok(())
            }
            crate::utils::context::ProjectType::Rust => {
                println!("[INFO] No aplicable a proyectos Rust.");
                Ok(())
            }
        },
        // Firmas: una sola ruta para toda la tarjeta unificada de `signer`
        // (crear/importar/manual, estado, huellas, respaldos, secretos de CI y
        // verificación de APK). Los nombres antiguos quedan como alias.
        "sign" | "signing" | "sha1" | "keystore" => tasks::signer::manage(ctx),
        "version" => tasks::version::bump(ctx),
        // Git: una sola ruta para la tarjeta unificada (`git::manage`): init,
        // .gitignore y conexión con el remoto. Los nombres antiguos son alias.
        "git" | "git-init" | "gitignore" => tasks::git::manage(ctx),
        "whatsapp" => tasks::whatsapp::build(ctx),
        "terminal" => tasks::terminal::configure(ctx),
        "flutter-profile" => {
            println!("\n[INFO] Selecciona la plataforma para Profile:\n");
            if utils::platform::IS_WINDOWS {
                println!("  1. Windows");
            } else {
                println!("  1. Linux");
            }
            println!("  2. Android");
            print!("\nOpcion: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            match input.trim() {
                "1" => tasks::flutter::profile_windows(ctx),
                "2" => tasks::flutter::profile_android(ctx),
                _ => {
                    println!("[ERROR] Opcion no valida.");
                    Ok(())
                }
            }
        }
        _ => anyhow::bail!("Tarea desconocida: '{}'. Usa --help para ver las opciones.", task),
    }
}
