#[cfg(not(windows))]
compile_error!("Compass solo tiene soporte para Windows.");

mod container;
mod menu;
mod tasks;
mod theme;
mod utils;

use anyhow::Result;
use clap::Parser;
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    name = "compass",
    about = "Compass - Tauri/Flutter/Kotlin/Go/Python/Rust para Windows",
    version
)]
struct Cli {
    #[arg(
        short,
        long,
        help = "Ejecutar tarea directamente (ej: windows-dev, clean)"
    )]
    task: Option<String>,

    #[arg(short, long, help = "Ruta del proyecto (se autodetecta por defecto)")]
    path: Option<String>,

    /// Elegir la carpeta del proyecto con un navegador, en vez de autodetectar.
    ///
    #[arg(long, help = "Elegir la carpeta del proyecto con un navegador")]
    pick: bool,

    #[arg(
        long,
        help = "Registrar eventos del menú TUI en <tmpdir>/compass-tui.log"
    )]
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
    //                     cae al navegador de proyectos.
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

    if cli.task.as_deref() == Some("context-gitignore") {
        return tasks::git::add_context_gitignore_from_dir(&dir);
    }

    // ── Instancia única por directorio ──
    //
    // Sólo aplica a la TUI (sin --task). Los procesos hijo que ejecutan tareas
    // (`compass --task ...`) no deben bloquearse: si lo hicieran, las cards del
    // menú principal dejarían de funcionar porque el lock del padre ya está
    // tomado.
    let _instance_lock = if cli.task.is_none() {
        match utils::instance_lock::try_acquire(&dir) {
            Ok(Some(lock)) => Some(lock),
            Ok(None) => {
                // Ya hay una instancia corriendo: el banner ya se mostró.
                return Ok(());
            }
            Err(e) => {
                eprintln!("[WARN] No se pudo verificar instancia única: {}", e);
                None
            }
        }
    } else {
        None
    };

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
    // fijarlo aquí con el nombre del directorio, la pestaña muestra el
    // proyecto en vez de la ruta del binario. El acento de pestaña (Windows
    // Terminal) se deriva del path para distinguir instancias concurrentes.
    crate::utils::cmd::set_console_title(&ctx.window_title());
    crate::utils::cmd::set_tab_color(crate::utils::platform::tab_accent(&ctx.script_dir));
    match task {
        "windows-dev" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::windows::dev(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::run_windows(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::run_windows(ctx),
            crate::utils::context::ProjectType::Rust => tasks::rust::run(ctx),
            crate::utils::context::ProjectType::Go => tasks::go::run(ctx),
            crate::utils::context::ProjectType::GoWails => tasks::gowails::dev(ctx),
            crate::utils::context::ProjectType::Python => tasks::python::run(ctx),
        },
        "android-dev" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::dev(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::run_android(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::run_android(ctx),
            crate::utils::context::ProjectType::Rust => {
                anyhow::bail!("No aplicable a proyectos Rust")
            }
            crate::utils::context::ProjectType::Go => {
                anyhow::bail!("Los proyectos Go puros no tienen tarea Android")
            }
            crate::utils::context::ProjectType::GoWails => tasks::gowails::install_debug(ctx),
            crate::utils::context::ProjectType::Python => {
                anyhow::bail!("Python no tiene tarea Android")
            }
        },
        "android-hmr" => match ctx.project_type {
            crate::utils::context::ProjectType::GoWails => tasks::gowails::dev_android(ctx),
            crate::utils::context::ProjectType::Python => tasks::python::reload(ctx),
            _ => anyhow::bail!("HMR sólo disponible para Go + Wails v3 o Python ASGI"),
        },
        "go-hmr" => match ctx.project_type {
            crate::utils::context::ProjectType::Go => tasks::go::reload(ctx),
            _ => anyhow::bail!("La recarga con Air sólo está disponible para proyectos Go puros"),
        },
        "windows-build" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::windows::build(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_windows(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_windows(ctx),
            crate::utils::context::ProjectType::Rust => tasks::rust::build(ctx),
            crate::utils::context::ProjectType::Go => tasks::go::build(ctx),
            crate::utils::context::ProjectType::GoWails => tasks::gowails::build_desktop(ctx),
            crate::utils::context::ProjectType::Python => tasks::python::build(ctx),
        },
        // Submenús TUI de release por arquitectura.
        "windows-release" => tasks::release::manage_windows(ctx),
        "android-release" => tasks::release::manage_android(ctx),
        "android-aab" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_aab(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_aab(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_aab(ctx),
            crate::utils::context::ProjectType::GoWails => tasks::gowails::build_aab(ctx),
            crate::utils::context::ProjectType::Rust => {
                anyhow::bail!("No aplicable a proyectos Rust")
            }
            crate::utils::context::ProjectType::Go => {
                anyhow::bail!("Los proyectos Go puros no tienen compilación Android")
            }
            crate::utils::context::ProjectType::Python => {
                anyhow::bail!("Python no tiene compilación Android")
            }
        },
        "android-arm64" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_arm64(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_apk(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_apk(ctx),
            crate::utils::context::ProjectType::Rust => {
                anyhow::bail!("No aplicable a proyectos Rust")
            }
            crate::utils::context::ProjectType::Go => {
                anyhow::bail!("Los proyectos Go puros no tienen compilación Android")
            }
            crate::utils::context::ProjectType::GoWails => tasks::gowails::build_apk(ctx),
            crate::utils::context::ProjectType::Python => {
                anyhow::bail!("Python no tiene compilación Android")
            }
        },
        "android-armv7" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_armv7(ctx),
            crate::utils::context::ProjectType::Flutter => {
                anyhow::bail!("No aplicable a proyectos Flutter")
            }
            crate::utils::context::ProjectType::Kotlin => {
                anyhow::bail!("No aplicable a proyectos Kotlin")
            }
            crate::utils::context::ProjectType::Rust => {
                anyhow::bail!("No aplicable a proyectos Rust")
            }
            crate::utils::context::ProjectType::Go => {
                anyhow::bail!("Los proyectos Go puros no tienen compilación Android")
            }
            crate::utils::context::ProjectType::GoWails => tasks::gowails::build_apk_armv7(ctx),
            crate::utils::context::ProjectType::Python => {
                anyhow::bail!("Python no tiene compilación Android")
            }
        },
        "android-all" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_all(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_aab(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_aab(ctx),
            crate::utils::context::ProjectType::Rust => {
                anyhow::bail!("No aplicable a proyectos Rust")
            }
            crate::utils::context::ProjectType::Go => {
                anyhow::bail!("Los proyectos Go puros no tienen compilación Android")
            }
            crate::utils::context::ProjectType::GoWails => tasks::gowails::build_aab(ctx),
            crate::utils::context::ProjectType::Python => {
                anyhow::bail!("Python no tiene compilación Android")
            }
        },
        "android-install-debug" => match ctx.project_type {
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::install_debug(ctx),
            crate::utils::context::ProjectType::GoWails => tasks::gowails::install_debug(ctx),
            _ => anyhow::bail!("No aplicable a proyectos Kotlin (solo Gradle)"),
        },
        // Limpieza: una sola ruta para el menú unificado de `cleanup` (rápida,
        // completa y reporte de tamaños). Los nombres antiguos siguen valiendo
        // como alias.
        "clean" | "clean-cache" | "clean-project" => tasks::cleanup::manage(ctx),
        "python-test" => match ctx.project_type {
            crate::utils::context::ProjectType::Python => tasks::python::test(ctx),
            _ => anyhow::bail!("pytest sólo está disponible para proyectos Python"),
        },
        "go-test" => match ctx.project_type {
            crate::utils::context::ProjectType::Go => tasks::go::test(ctx),
            _ => anyhow::bail!("go test sólo está disponible para proyectos Go puros"),
        },
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
            crate::utils::context::ProjectType::GoWails => tasks::gowails::debug_android(ctx),
            crate::utils::context::ProjectType::Python => {
                println!("[INFO] Python no tiene depuración Android.");
                Ok(())
            }
            crate::utils::context::ProjectType::Rust => {
                println!("[INFO] No aplicable a proyectos Rust.");
                Ok(())
            }
            crate::utils::context::ProjectType::Go => {
                println!("[INFO] Los proyectos Go puros no tienen depuración Android.");
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
        "context-gitignore" => tasks::git::add_context_gitignore(ctx),
        "whatsapp" => tasks::whatsapp::build(ctx),
        // Empaquetado del instalador (sólo Go + Wails: NSIS/MSIX).
        "package" | "windows-package" => match ctx.project_type {
            crate::utils::context::ProjectType::GoWails => tasks::gowails::package_desktop(ctx),
            _ => anyhow::bail!("No aplicable a este tipo de proyecto"),
        },
        "terminal" => tasks::terminal::configure(ctx),
        // Respaldo y restauración del entorno de desarrollo (scripts embebidos
        // del proyecto Variables Backupper). Alias por comodidad en la CLI.
        "environment" | "env" | "env-backup" | "env-restore" => tasks::environment::manage(ctx),
        "flutter-profile" => {
            println!("\n[INFO] Selecciona la plataforma para Profile:\n");
            println!("  1. Windows");
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
        _ => anyhow::bail!(
            "Tarea desconocida: '{}'. Usa --help para ver las opciones.",
            task
        ),
    }
}
