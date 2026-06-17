use inquire::Select;
use crate::tasks;
use crate::utils::cmd;
use crate::utils::context::ProjectContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    WindowsDev,
    AndroidDev,
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildAll,
    CleanCache,
    DebugAndroid,
    ConfigureSigning,
    CleanProject,
    CreateGitignore,
    ManageKeystore,
    BuildWhatsApp,
    ConfigureTerminal,
    Exit,
}

impl std::fmt::Display for MenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::WindowsDev => "▲ Windows (Desarrollo)",
            Self::AndroidDev => "▲ Android (Desarrollo)",
            Self::WindowsBuild => "▲ Windows Build (Release)",
            Self::AndroidBuildArm64 => "◆ Android Build (Release - arm64) [Rápido]",
            Self::AndroidBuildAll => "◆ Android Build (Release - Todas)",
            Self::CleanCache => "■ Limpiar cache y dependencias",
            Self::DebugAndroid => "■ Depurar Android (Chrome Inspect)",
            Self::ConfigureSigning => "■ Configurar firmas Android (Release)",
            Self::CleanProject => "■ Limpiar proyecto para portabilidad",
            Self::CreateGitignore => "● Crear .gitignore para Rust/Tauri",
            Self::ManageKeystore => "● Gestionar llaves (keystore)",
            Self::BuildWhatsApp => "● Compilar motor de WhatsApp",
            Self::ConfigureTerminal => "● Configurar terminal (autocompletado TAB)",
            Self::Exit => "✕ Salir",
        };
        write!(f, "{}", s)
    }
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    loop {
        cmd::clear_screen();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║     GESTOR DE DESARROLLO - TAURI / RUST         ║");
        println!("║              developing-runner v0.1.0            ║");
        println!("╚══════════════════════════════════════════════════╝");
        if ctx.android_home.is_none() {
            println!("⚠ ADVERTENCIA: ANDROID_HOME no está configurada.\n");
        }
        println!();

        let items = vec![
            MenuItem::WindowsDev,
            MenuItem::AndroidDev,
            MenuItem::WindowsBuild,
            MenuItem::AndroidBuildArm64,
            MenuItem::AndroidBuildAll,
            MenuItem::CleanCache,
            MenuItem::DebugAndroid,
            MenuItem::ConfigureSigning,
            MenuItem::CleanProject,
            MenuItem::CreateGitignore,
            MenuItem::ManageKeystore,
            MenuItem::BuildWhatsApp,
            MenuItem::ConfigureTerminal,
            MenuItem::Exit,
        ];

        let selection = Select::new("¿Qué deseas hacer?", items)
            .with_vim_mode(true)
            .with_page_size(14)
            .prompt()?;

        cmd::clear_screen();

        match selection {
            MenuItem::Exit => break,
            MenuItem::WindowsDev => tasks::windows::dev(ctx)?,
            MenuItem::AndroidDev => tasks::android::dev(ctx)?,
            MenuItem::WindowsBuild => tasks::windows::build(ctx)?,
            MenuItem::AndroidBuildArm64 => tasks::android::build_arm64(ctx)?,
            MenuItem::AndroidBuildAll => tasks::android::build_all(ctx)?,
            MenuItem::CleanCache => tasks::cleanup::clean_cache(ctx)?,
            MenuItem::DebugAndroid => tasks::android::debug(ctx)?,
            MenuItem::ConfigureSigning => tasks::android::configure_signing(ctx)?,
            MenuItem::CleanProject => tasks::cleanup::clean_project(ctx)?,
            MenuItem::CreateGitignore => tasks::gitignore::create(ctx)?,
            MenuItem::ManageKeystore => tasks::keystore::manage(ctx)?,
            MenuItem::BuildWhatsApp => tasks::whatsapp::build(ctx)?,
            MenuItem::ConfigureTerminal => tasks::terminal::configure(ctx)?,
        }
    }
    Ok(())
}
