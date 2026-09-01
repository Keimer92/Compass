mod menu;
mod tasks;
mod utils;

use clap::Parser;
use anyhow::Result;

#[derive(Parser)]
#[command(
    name = "code-runner",
    about = "Code Runner - Multiplataforma (Tauri/Flutter/KMP)",
    version = "0.1.0",
)]
struct Cli {
    #[arg(short, long, help = "Ejecutar tarea directamente (ej: windows-dev, clean)")]
    task: Option<String>,

    #[arg(short, long, help = "Ruta del proyecto (se autodetecta por defecto)")]
    path: Option<String>,

    #[arg(long, help = "Registrar eventos del menú TUI en %TEMP%\\code-runner-tui.log")]
    trace: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(task) = &cli.task {
        if task.starts_with("context-menu") || task.starts_with("opencode") || task.starts_with("cline") || task.starts_with("kilo") {
            return run_global_task(task);
        }
    }

    let ctx = if let Some(p) = &cli.path {
        let dir = std::path::PathBuf::from(p);
        if !dir.exists() {
            anyhow::bail!("La ruta especificada no existe: {}", p);
        }
        std::env::set_current_dir(&dir)?;
        utils::context::detect()?
    } else {
        utils::context::detect()?
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
        _ => anyhow::bail!("Tarea desconocida: {}", task),
    }
}

fn run_task_direct(ctx: &crate::utils::context::ProjectContext, task: &str) -> Result<()> {
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
        "clean-cache" => tasks::cleanup::clean_cache(ctx),
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
        "sign" => tasks::android::configure_signing(ctx),
        "sha1" => tasks::sha1::show(ctx),
        "clean-project" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::cleanup::clean_project(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::clean(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::clean(ctx),
            crate::utils::context::ProjectType::Rust => tasks::rust::clean(ctx),
        },
        "version" => tasks::version::bump(ctx),
        "gitignore" => tasks::gitignore::create(ctx),
        "git-init" => tasks::git::init(ctx),
        "keystore" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::keystore::manage(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::android::configure_signing(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::android::configure_signing(ctx),
            crate::utils::context::ProjectType::Rust => anyhow::bail!("No aplicable a proyectos Rust"),
        },
        "launch-emulator" => tasks::android::launch(ctx),
        "whatsapp" => tasks::whatsapp::build(ctx),
        "terminal" => tasks::terminal::configure(ctx),
        _ => anyhow::bail!("Tarea desconocida: '{}'. Usa --help para ver las opciones.", task),
    }
}
