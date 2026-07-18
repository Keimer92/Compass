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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(task) = &cli.task {
        if task == "context-menu" || task == "context-menu-install" {
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
        menu::run(&ctx)?;
    }

    Ok(())
}

fn run_global_task(task: &str) -> Result<()> {
    match task {
        "context-menu" => tasks::context_menu::toggle(),
        "context-menu-install" => tasks::context_menu::install(),
        _ => anyhow::bail!("Tarea desconocida: {}", task),
    }
}

fn run_task_direct(ctx: &crate::utils::context::ProjectContext, task: &str) -> Result<()> {
    match task {
        "windows-dev" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::windows::dev(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::run_windows(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::run_windows(ctx),
        },
        "android-dev" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::dev(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::run_android(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::run_android(ctx),
        },
        "windows-build" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::windows::build(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_windows(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_windows(ctx),
        },
        "android-arm64" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_arm64(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_apk(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_apk(ctx),
        },
        "android-all" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::android::build_all(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::build_aab(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::build_aab(ctx),
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
        },
        "sign" => tasks::android::configure_signing(ctx),
        "clean-project" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::cleanup::clean_project(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::flutter::clean(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::kotlin::clean(ctx),
        },
        "gitignore" => tasks::gitignore::create(ctx),
        "git-init" => tasks::git::init(ctx),
        "keystore" => match ctx.project_type {
            crate::utils::context::ProjectType::Tauri => tasks::keystore::manage(ctx),
            crate::utils::context::ProjectType::Flutter => tasks::android::configure_signing(ctx),
            crate::utils::context::ProjectType::Kotlin => tasks::android::configure_signing(ctx),
        },
        "whatsapp" => tasks::whatsapp::build(ctx),
        "terminal" => tasks::terminal::configure(ctx),
        "context-menu" => tasks::context_menu::toggle(),
        "context-menu-install" => tasks::context_menu::install(),
        _ => anyhow::bail!("Tarea desconocida: '{}'. Usa --help para ver las opciones.", task),
    }
}
