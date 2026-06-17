use inquire::Select;
use crate::tasks;
use crate::utils::cmd;
use crate::utils::context::ProjectContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    // Desarrollo
    WindowsDev,
    AndroidDev,
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildAll,
    // Mantenimiento
    CleanCache,
    DebugAndroid,
    ConfigureSigning,
    CleanProject,
    // Otros
    CreateGitignore,
    InitGit,
    ManageKeystore,
    BuildWhatsApp,
    ConfigureTerminal,
    Exit,
}

impl std::fmt::Display for MenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (icon, section, label) = match self {
            Self::WindowsDev       => ("🖥", "DEV", "Windows (Desarrollo)"),
            Self::AndroidDev       => ("📱", "DEV", "Android (Desarrollo)"),
            Self::WindowsBuild     => ("📦", "DEV", "Windows Build (Release)"),
            Self::AndroidBuildArm64=> ("🚀", "DEV", "Android Build (Release - arm64)"),
            Self::AndroidBuildAll  => ("🚀", "DEV", "Android Build (Release - Todas)"),
            Self::CleanCache       => ("🧹", "MNT", "Limpiar cache y dependencias"),
            Self::DebugAndroid     => ("🔍", "MNT", "Depurar Android (Chrome Inspect)"),
            Self::ConfigureSigning  => ("🔑", "MNT", "Configurar firmas Android (Release)"),
            Self::CleanProject     => ("🧼", "MNT", "Limpiar proyecto para portabilidad"),
            Self::CreateGitignore  => ("📄", "CFG", "Crear .gitignore"),
            Self::InitGit          => ("🔧", "CFG", "Inicializar Git + primer commit"),
            Self::ManageKeystore   => ("🗝",  "CFG", "Gestionar llaves (keystore)"),
            Self::BuildWhatsApp    => ("💬", "CFG", "Compilar motor de WhatsApp"),
            Self::ConfigureTerminal=> ("⚙",  "CFG", "Configurar terminal (autocompletado)"),
            Self::Exit             => ("✕",  "",    "Salir"),
        };
        if section.is_empty() {
            write!(f, "  {}  {}", icon, label)
        } else {
            write!(f, "  {}  [{}]  {}", icon, section, label)
        }
    }
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    loop {
        cmd::clear_screen();
        let project_name = ctx.script_dir.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or(std::borrow::Cow::Borrowed("Proyecto"));
        println!("╭{}╮", "─".repeat(48));
        println!("│  ◈  developing-runner  v0.1.0{:>27}│", " ");
        println!("│     Proyecto: {:<38}│", project_name);
        if ctx.android_home.is_none() {
            println!("│{:^50}│", "⚠  ANDROID_HOME no configurada");
        }
        println!("├{}┤", "─".repeat(48));
        println!("│  {:^46}  │", "DESARROLLO");
        println!("╰{}╯", "─".repeat(48));

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
            MenuItem::InitGit,
            MenuItem::ManageKeystore,
            MenuItem::BuildWhatsApp,
            MenuItem::ConfigureTerminal,
            MenuItem::Exit,
        ];

        let selection = Select::new("  ¿Qué acción deseas ejecutar?", items)
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
            MenuItem::InitGit => tasks::git::init(ctx)?,
            MenuItem::ManageKeystore => tasks::keystore::manage(ctx)?,
            MenuItem::BuildWhatsApp => tasks::whatsapp::build(ctx)?,
            MenuItem::ConfigureTerminal => tasks::terminal::configure(ctx)?,
        }
    }
    Ok(())
}
