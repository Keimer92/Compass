//! Submenús de release por arquitectura (Windows y Android).
//!
//! Reutilizan [`crate::tasks::submenu`] para que las cards tengan el mismo
//! aspecto que el menú padre: cabecera, fila de sección, rejilla adaptativa,
//! halo de hover y pie. Cada submenú compila **una** arquitectura o todas en
//! serie; el revelado y la pausa se hacen una sola vez al final del lote.

use anyhow::Result;
use std::path::PathBuf;

use crate::menu;
use crate::tasks::submenu::{Submenu, SubmenuAction, SubmenuItem};
use crate::tasks::{android, flutter, go, gowails, kotlin, python, rust, windows};
use crate::theme;
use crate::utils::{
    cmd,
    context::{ProjectContext, ProjectType},
    ui,
};

const TITLE_WINDOWS: &str = "RELEASE · WINDOWS";
const TITLE_ANDROID: &str = "RELEASE · ANDROID";

const ICON_WINDOWS: &str = "\u{1f5a5}";
const ICON_ANDROID: &str = "\u{1f4f1}";
const ICON_UNIVERSAL: &str = "\u{1f4e6}";
const ICON_ALL: &str = "\u{2705}";
const ICON_BACK: &str = "\u{21a9}";

/// Arquitectura de escritorio que se puede compilar por separado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsArch {
    X86,
    X64,
    Arm64,
    All,
}

/// Arquitectura/entrega de Android que se puede compilar por separado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidArch {
    Arm64,
    Armv7,
    Universal,
    All,
}

impl AndroidArch {
    fn label(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::Armv7 => "armv7",
            Self::Universal => "universal",
            Self::All => "todas",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsChoice {
    Arch(WindowsArch),
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AndroidChoice {
    Arch(AndroidArch),
    Back,
}

/// Cómo se muestra el resultado de un lote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reveal {
    /// Resaltar el último artefacto en Explorer.
    LastArtifact,
    /// Abrir la carpeta `bin/` (Go + Wails deja ahí todos sus paquetes).
    BinFolder,
}

fn windows_supported(kind: ProjectType) -> bool {
    matches!(
        kind,
        ProjectType::Tauri | ProjectType::Rust | ProjectType::Go | ProjectType::GoWails
    )
}

fn android_supported(kind: ProjectType) -> bool {
    matches!(
        kind,
        ProjectType::Tauri | ProjectType::Flutter | ProjectType::Kotlin | ProjectType::GoWails
    )
}

/// Triple de Rust para cada arquitectura de Windows (`None` para `All`).
fn windows_triple(arch: WindowsArch) -> Option<&'static str> {
    match arch {
        WindowsArch::X86 => Some("i686-pc-windows-msvc"),
        WindowsArch::X64 => Some("x86_64-pc-windows-msvc"),
        WindowsArch::Arm64 => Some("aarch64-pc-windows-msvc"),
        WindowsArch::All => None,
    }
}

/// Sufijo del nombre del artefacto (x64 no lleva sufijo).
fn windows_label(arch: WindowsArch) -> &'static str {
    match arch {
        WindowsArch::X86 => "x86",
        // x64 es la entrega "normal": no lleva sufijo.
        WindowsArch::X64 | WindowsArch::All => "",
        // El nombre visible de la card es ARM64, pero el archivo lleva `arm64`.
        WindowsArch::Arm64 => "arm64",
    }
}

fn go_arch(arch: WindowsArch) -> Option<go::GoArch> {
    match arch {
        WindowsArch::X86 => Some(go::GoArch::X86),
        WindowsArch::X64 => Some(go::GoArch::X64),
        WindowsArch::Arm64 => Some(go::GoArch::Arm64),
        WindowsArch::All => None,
    }
}

fn desktop_arch(arch: WindowsArch) -> Option<gowails::DesktopArch> {
    match arch {
        WindowsArch::X86 => Some(gowails::DesktopArch::X86),
        WindowsArch::X64 => Some(gowails::DesktopArch::X64),
        WindowsArch::Arm64 => Some(gowails::DesktopArch::Arm64),
        WindowsArch::All => None,
    }
}

fn windows_item(
    icon: &'static str,
    label: &'static str,
    description: &'static str,
    color_key: &'static str,
) -> SubmenuItem {
    SubmenuItem {
        icon,
        label,
        description,
        color_key,
    }
}

fn back_item() -> SubmenuItem {
    windows_item(
        ICON_BACK,
        "Volver al menu principal",
        "Regresa al menu principal",
        "git",
    )
}

/// Entradas del submenú de Windows para el tipo de proyecto.
///
/// Flutter/Kotlin/Python no lo tienen: su release de escritorio sólo compila el
/// host, así que la card delega en el build de siempre.
pub(crate) fn windows_entries(ctx: &ProjectContext) -> Vec<(WindowsChoice, SubmenuItem)> {
    if !windows_supported(ctx.project_type) {
        return Vec::new();
    }
    vec![
        (
            WindowsChoice::Arch(WindowsArch::X86),
            windows_item(
                ICON_WINDOWS,
                "Windows x86",
                "32 bits · PCs y VMs antiguas",
                "build",
            ),
        ),
        (
            WindowsChoice::Arch(WindowsArch::X64),
            windows_item(
                ICON_WINDOWS,
                "Windows x64",
                "64 bits · uso general",
                "run",
            ),
        ),
        (
            WindowsChoice::Arch(WindowsArch::Arm64),
            windows_item(
                ICON_WINDOWS,
                "Windows ARM64",
                "Snapdragon X · bajo consumo",
                "reload",
            ),
        ),
        (
            WindowsChoice::Arch(WindowsArch::All),
            windows_item(
                ICON_ALL,
                "Todos",
                "Compila x86 + x64 + ARM64",
                "profile",
            ),
        ),
        (WindowsChoice::Back, back_item()),
    ]
}

/// Entradas del submenú de Android para el tipo de proyecto.
///
/// Kotlin sin `jniLibs` no tiene APK por ABI: las tres serían idénticas, así que
/// sólo se ofrece la universal.
pub(crate) fn android_entries(ctx: &ProjectContext) -> Vec<(AndroidChoice, SubmenuItem)> {
    if !android_supported(ctx.project_type) {
        return Vec::new();
    }

    if ctx.project_type == ProjectType::Kotlin
        && !kotlin::has_native_libs(&crate::utils::context::kotlin_android_dir(ctx))
    {
        return vec![
            (
                AndroidChoice::Arch(AndroidArch::Universal),
                windows_item(
                    ICON_UNIVERSAL,
                    "Android universal",
                    "Sin libs nativas · sirve para todas las ABIs",
                    "apk",
                ),
            ),
            (AndroidChoice::Back, back_item()),
        ];
    }

    vec![
        (
            AndroidChoice::Arch(AndroidArch::Arm64),
            windows_item(
                ICON_ANDROID,
                "Android arm64",
                "arm64-v8a · moviles modernos",
                "android",
            ),
        ),
        (
            AndroidChoice::Arch(AndroidArch::Armv7),
            windows_item(
                ICON_ANDROID,
                "Android armv7",
                "armeabi-v7a · equipos antiguos",
                "apk32",
            ),
        ),
        (
            AndroidChoice::Arch(AndroidArch::Universal),
            windows_item(
                ICON_UNIVERSAL,
                "Android universal",
                "Una APK con todas las ABIs",
                "apk",
            ),
        ),
        (
            AndroidChoice::Arch(AndroidArch::All),
            windows_item(
                ICON_ALL,
                "Todos",
                "Compila las tres APK en serie",
                "aab",
            ),
        ),
        (AndroidChoice::Back, back_item()),
    ]
}

/// Submenú de release de escritorio. En Flutter/Kotlin/Python se delega en el
/// build de escritorio de siempre (sólo host).
pub fn manage_windows(ctx: &ProjectContext) -> Result<()> {
    match ctx.project_type {
        ProjectType::Tauri
        | ProjectType::Rust
        | ProjectType::Go
        | ProjectType::GoWails => run_menu(
            ctx,
            TITLE_WINDOWS,
            windows_entries(ctx),
            false,
            Reveal::LastArtifact,
            |choice| choice == WindowsChoice::Back,
            |choice| match choice {
                WindowsChoice::Back => Ok(Vec::new()),
                WindowsChoice::Arch(arch) => windows_artifacts(ctx, arch),
            },
        ),
        ProjectType::Flutter => flutter::build_windows(ctx),
        ProjectType::Kotlin => kotlin::build_windows(ctx),
        ProjectType::Python => python::build(ctx),
    }
}

/// Submenú de release de Android (APK por ABI, universal y AAB aparte).
pub fn manage_android(ctx: &ProjectContext) -> Result<()> {
    if !android_supported(ctx.project_type) {
        cmd::clear_screen();
        println!("[INFO] Los builds de Android no aplican a este tipo de proyecto.");
        ui::pause();
        return Ok(());
    }

    let reveal = if ctx.project_type == ProjectType::GoWails {
        Reveal::BinFolder
    } else {
        Reveal::LastArtifact
    };

    run_menu(
        ctx,
        TITLE_ANDROID,
        android_entries(ctx),
        true,
        reveal,
        |choice| choice == AndroidChoice::Back,
        |choice| match choice {
            AndroidChoice::Back => Ok(Vec::new()),
            AndroidChoice::Arch(arch) => android_artifacts(ctx, arch),
        },
    )
}

/// Bucle canónico de los submenús (mismo que `signer::manage`): restaurar la
/// consola para ejecutar y volver a montar el TUI después, sin cerrar el
/// submenú si el build falla.
fn run_menu<T: Copy>(
    ctx: &ProjectContext,
    title: &'static str,
    entries: Vec<(T, SubmenuItem)>,
    warn_android: bool,
    reveal: Reveal,
    is_back: impl Fn(T) -> bool,
    exec: impl Fn(T) -> Result<Vec<PathBuf>>,
) -> Result<()> {
    let theme = theme::load();
    let mut menu = Submenu::new(title, "DEV", entries).with_android_warning(warn_android);

    let mut terminal = menu::setup_terminal()?;
    cmd::set_console_title(&ctx.window_title());

    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| menu.draw(f, f.area(), ctx, theme))?;
            match menu.handle_event()? {
                SubmenuAction::Back => return Ok(()),
                SubmenuAction::Continue => {}
                SubmenuAction::Run(item) => {
                    if is_back(item) {
                        return Ok(());
                    }
                    menu::restore_terminal(&mut terminal)?;
                    let outcome = exec(item);
                    terminal = menu::setup_terminal()?;
                    cmd::set_console_title(&ctx.window_title());
                    match outcome {
                        Ok(artifacts) => finish(ctx, &artifacts, reveal),
                        Err(error) => {
                            println!();
                            println!("{error}");
                            println!();
                            ui::pause();
                        }
                    }
                }
            }
        }
    })();

    let _ = menu::restore_terminal(&mut terminal);
    result
}

/// Lista lo generado, revela el resultado y espera una sola vez por lote.
fn finish(ctx: &ProjectContext, artifacts: &[PathBuf], reveal: Reveal) {
    if !artifacts.is_empty() {
        println!();
        println!("[OK] Artefactos generados:");
        for artifact in artifacts {
            println!("  - {}", artifact.display());
        }
        match reveal {
            Reveal::LastArtifact => {
                if let Some(last) = artifacts.last() {
                    cmd::show_artifact(last);
                }
            }
            Reveal::BinFolder => cmd::open_folder(&ctx.script_dir.join("bin")),
        }
    }
    ui::pause_for(10);
}

fn windows_artifacts(ctx: &ProjectContext, arch: WindowsArch) -> Result<Vec<PathBuf>> {
    if arch == WindowsArch::All {
        let mut artifacts = Vec::new();
        for arch in [WindowsArch::X86, WindowsArch::X64, WindowsArch::Arm64] {
            artifacts.extend(windows_artifacts(ctx, arch)?);
        }
        return Ok(artifacts);
    }

    match ctx.project_type {
        ProjectType::Tauri => {
            let triple = windows_triple(arch)
                .ok_or_else(|| anyhow::anyhow!("[ERROR] Arquitectura sin triple."))?;
            Ok(vec![windows::build_arch(
                ctx,
                triple,
                windows_label(arch),
            )?])
        }
        ProjectType::Rust => {
            let triple = windows_triple(arch)
                .ok_or_else(|| anyhow::anyhow!("[ERROR] Arquitectura sin triple."))?;
            Ok(vec![rust::build_arch(ctx, triple, windows_label(arch))?])
        }
        ProjectType::Go => {
            let target = go_arch(arch)
                .ok_or_else(|| anyhow::anyhow!("[ERROR] Arquitectura sin GOARCH."))?;
            Ok(vec![go::build_arch(ctx, target)?])
        }
        ProjectType::GoWails => {
            let target = desktop_arch(arch)
                .ok_or_else(|| anyhow::anyhow!("[ERROR] Arquitectura sin equivalente."))?;
            gowails::build_desktop_arch(ctx, target)
        }
        _ => anyhow::bail!(
            "[ERROR] {} no compila escritorio por arquitectura.",
            ctx.project_type.label()
        ),
    }
}

fn android_artifacts(ctx: &ProjectContext, arch: AndroidArch) -> Result<Vec<PathBuf>> {
    if arch == AndroidArch::All {
        // Los lotes nativos hacen una sola pasada; Tauri no tiene splits, así
        // que compila arquitectura a arquitectura.
        return match ctx.project_type {
            ProjectType::Tauri => {
                let mut artifacts = Vec::new();
                for arch in [
                    AndroidArch::Arm64,
                    AndroidArch::Armv7,
                    AndroidArch::Universal,
                ] {
                    artifacts.extend(android_artifacts(ctx, arch)?);
                }
                Ok(artifacts)
            }
            ProjectType::Flutter => flutter::build_apks_all(ctx),
            ProjectType::Kotlin => {
                let module = crate::utils::context::kotlin_android_dir(ctx);
                if kotlin::has_native_libs(&module) {
                    kotlin::build_apks_all(ctx)
                } else {
                    Ok(vec![kotlin::build_apk_universal(ctx)?])
                }
            }
            ProjectType::GoWails => {
                let apks = gowails::build_release_apks(ctx)?;
                if let Some(universal) = apks.last() {
                    gowails::install_apk(ctx, universal)?;
                }
                Ok(apks)
            }
            kind => anyhow::bail!("[ERROR] {} no compila APK.", kind.label()),
        };
    }

    match (ctx.project_type, arch) {
        (ProjectType::Tauri, AndroidArch::Arm64) => {
            Ok(vec![android::build_arch(ctx, "aarch64", "arm64")?])
        }
        (ProjectType::Tauri, AndroidArch::Armv7) => {
            Ok(vec![android::build_arch(ctx, "armv7", "armv7")?])
        }
        (ProjectType::Tauri, AndroidArch::Universal) => Ok(vec![android::build_universal(ctx)?]),

        (ProjectType::Flutter, AndroidArch::Arm64) => {
            Ok(vec![flutter::build_apk_arch(ctx, "android-arm64", "arm64")?])
        }
        (ProjectType::Flutter, AndroidArch::Armv7) => {
            Ok(vec![flutter::build_apk_arch(ctx, "android-arm", "armv7")?])
        }
        (ProjectType::Flutter, AndroidArch::Universal) => {
            Ok(vec![flutter::build_apk_universal(ctx)?])
        }

        (ProjectType::Kotlin, AndroidArch::Arm64) => {
            Ok(vec![kotlin::build_apk_arch(ctx, "arm64-v8a", "arm64")?])
        }
        (ProjectType::Kotlin, AndroidArch::Armv7) => Ok(vec![kotlin::build_apk_arch(
            ctx,
            "armeabi-v7a",
            "armv7",
        )?]),
        (ProjectType::Kotlin, AndroidArch::Universal) => {
            Ok(vec![kotlin::build_apk_universal(ctx)?])
        }

        (ProjectType::GoWails, AndroidArch::Arm64) => {
            Ok(vec![gowails::build_apk_arch(ctx, gowails::ABI_ARM64)?])
        }
        (ProjectType::GoWails, AndroidArch::Armv7) => {
            Ok(vec![gowails::build_apk_arch(ctx, gowails::ABI_ARMV7)?])
        }
        (ProjectType::GoWails, AndroidArch::Universal) => {
            let apk = gowails::build_apk_universal(ctx)?;
            // El universal instala en cualquier teléfono (arm64 o armv7).
            gowails::install_apk(ctx, &apk)?;
            Ok(vec![apk])
        }

        (kind, arch) => anyhow::bail!(
            "[ERROR] {} no compila APK {}.",
            kind.label(),
            arch.label()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(kind: ProjectType) -> ProjectContext {
        ProjectContext {
            script_dir: std::path::PathBuf::from("C:/tmp/proyecto"),
            project_type: kind,
            android_home: Some(std::path::PathBuf::from("C:/Android")),
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        }
    }

    fn labels<T>(entries: &[(T, SubmenuItem)]) -> Vec<&'static str> {
        entries.iter().map(|(_, item)| item.label).collect()
    }

    #[test]
    fn windows_entries_are_offered_only_by_desktop_arch_types() {
        let expected = [
            "Windows x86",
            "Windows x64",
            "Windows ARM64",
            "Todos",
            "Volver al menu principal",
        ];
        for kind in [
            ProjectType::Tauri,
            ProjectType::Rust,
            ProjectType::Go,
            ProjectType::GoWails,
        ] {
            let entries = windows_entries(&ctx(kind));
            assert_eq!(labels(&entries), expected, "{kind:?}");
            assert_eq!(entries[3].0, WindowsChoice::Arch(WindowsArch::All));
            assert_eq!(entries.last().unwrap().0, WindowsChoice::Back);
        }
        for kind in [ProjectType::Flutter, ProjectType::Kotlin, ProjectType::Python] {
            assert!(windows_entries(&ctx(kind)).is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn android_entries_depend_on_the_toolchain() {
        let full = [
            "Android arm64",
            "Android armv7",
            "Android universal",
            "Todos",
            "Volver al menu principal",
        ];
        for kind in [
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::GoWails,
        ] {
            assert_eq!(labels(&android_entries(&ctx(kind))), full, "{kind:?}");
        }

        let kotlin = android_entries(&ctx(ProjectType::Kotlin));
        assert_eq!(
            labels(&kotlin),
            ["Android universal", "Volver al menu principal"]
        );
        assert_eq!(
            kotlin[0].1.description,
            "Sin libs nativas · sirve para todas las ABIs"
        );

        for kind in [ProjectType::Rust, ProjectType::Go, ProjectType::Python] {
            assert!(android_entries(&ctx(kind)).is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn android_entries_include_abis_when_kotlin_has_jnilibs() {
        let root = std::env::temp_dir().join(format!(
            "compass-release-jnilibs-{}",
            std::process::id()
        ));
        let jni = root
            .join("composeApp")
            .join("src")
            .join("main")
            .join("jniLibs")
            .join("arm64-v8a");
        std::fs::create_dir_all(&jni).unwrap();
        std::fs::write(jni.join("libdemo.so"), b"elf").unwrap();

        let mut context = ctx(ProjectType::Kotlin);
        context.script_dir = root.clone();
        let entries = android_entries(&context);
        assert_eq!(entries.len(), 5);
        assert_eq!(
            entries[3].0,
            AndroidChoice::Arch(AndroidArch::All)
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn card_color_keys_are_valid_and_distinct() {
        fn keys_of<T>(entries: &[(T, SubmenuItem)]) -> Vec<&'static str> {
            entries.iter().map(|(_, item)| item.color_key).collect()
        }

        let context = ctx(ProjectType::Tauri);
        for keys in [
            keys_of(&windows_entries(&context)),
            keys_of(&android_entries(&context)),
        ] {
            for key in &keys {
                assert!(
                    theme::CARD_KEYS.contains(key),
                    "clave de paleta desconocida: {key}"
                );
            }
            let mut sorted = keys.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), keys.len(), "claves repetidas: {keys:?}");
        }
    }

    #[test]
    fn windows_triples_and_goarchs_match_real_targets() {
        assert_eq!(windows_triple(WindowsArch::X86), Some("i686-pc-windows-msvc"));
        assert_eq!(
            windows_triple(WindowsArch::X64),
            Some("x86_64-pc-windows-msvc")
        );
        assert_eq!(
            windows_triple(WindowsArch::Arm64),
            Some("aarch64-pc-windows-msvc")
        );
        assert_eq!(windows_triple(WindowsArch::All), None);

        assert_eq!(windows_label(WindowsArch::X64), "");
        assert_eq!(windows_label(WindowsArch::X86), "x86");
        assert_eq!(windows_label(WindowsArch::Arm64), "arm64");

        assert_eq!(go_arch(WindowsArch::X86).unwrap().goarch(), "386");
        assert_eq!(go_arch(WindowsArch::X64).unwrap().goarch(), "amd64");
        assert_eq!(go_arch(WindowsArch::Arm64).unwrap().goarch(), "arm64");
        assert!(go_arch(WindowsArch::All).is_none());
    }

    #[test]
    fn back_is_the_last_entry_in_both_submenus() {
        let context = ctx(ProjectType::Tauri);
        let windows = windows_entries(&context);
        assert_eq!(windows.last().unwrap().0, WindowsChoice::Back);
        let android = android_entries(&context);
        assert_eq!(android.last().unwrap().0, AndroidChoice::Back);
    }
}
