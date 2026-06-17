mod menu;
mod tasks;
mod utils;

use clap::Parser;
use anyhow::Result;

#[derive(Parser)]
#[command(
    name = "developing-runner",
    about = "Gestor de desarrollo para proyectos Tauri / Rust",
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

    // If explicit path is provided, use it; otherwise auto-detect from current dir
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

fn run_task_direct(ctx: &crate::utils::context::ProjectContext, task: &str) -> Result<()> {
    match task {
        "windows-dev" => tasks::windows::dev(ctx),
        "android-dev" => tasks::android::dev(ctx),
        "windows-build" => tasks::windows::build(ctx),
        "android-arm64" => tasks::android::build_arm64(ctx),
        "android-all" => tasks::android::build_all(ctx),
        "clean-cache" => tasks::cleanup::clean_cache(ctx),
        "debug-android" => tasks::android::debug(ctx),
        "sign" => tasks::android::configure_signing(ctx),
        "clean-project" => tasks::cleanup::clean_project(ctx),
        "gitignore" => tasks::gitignore::create(ctx),
        "keystore" => tasks::keystore::manage(ctx),
        "whatsapp" => tasks::whatsapp::build(ctx),
        "terminal" => tasks::terminal::configure(ctx),
        _ => anyhow::bail!("Tarea desconocida: '{}'. Usa --help para ver las opciones.", task),
    }
}
