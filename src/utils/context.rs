use std::path::PathBuf;
use std::env;
use anyhow::{Context, Result};

use super::platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Tauri,
    Flutter,
    Kotlin,
    Rust,
}

impl ProjectType {
    pub fn whatsapp_path(&self, root: &PathBuf) -> PathBuf {
        match self {
            Self::Tauri => root.join("src-tauri").join("whatsapp-engine"),
            Self::Flutter | Self::Kotlin | Self::Rust => root.join("whatsapp_engine"),
        }
    }
}

pub struct ProjectContext {
    pub script_dir: PathBuf,
    pub project_type: ProjectType,
    pub android_home: Option<PathBuf>,
    pub emulator_path: Option<PathBuf>,
    pub ndk_home: Option<PathBuf>,
    pub adb_path: Option<PathBuf>,
    pub sdk_manager: Option<PathBuf>,
    pub has_node_modules: bool,
    pub has_android_project: bool,
    pub has_whatsapp_engine: bool,
}

impl ProjectContext {
    pub fn assert_android_sdk(&self) -> Result<()> {
        if self.android_home.is_none() {
            #[cfg(windows)]
            anyhow::bail!(
                "[ERROR] ANDROID_HOME no está configurado.\n\
                 Asegúrate de tener instalado Android SDK, o define ANDROID_HOME."
            );
            #[cfg(not(windows))]
            anyhow::bail!(
                "[ERROR] ANDROID_HOME no está configurado.\n\
                 Define la variable con: export ANDROID_HOME=$HOME/Android/Sdk"
            );
        }
        Ok(())
    }
}

/// Devuelve el directorio del módulo Android de un proyecto Kotlin Multiplatform.
/// La plantilla por defecto usa `composeApp`, pero algunos proyectos usan `app`
/// (p.ej. SongSync). Se usa para ubicar el `keystore.properties` / gradle / `MainActivity.kt`.
pub(crate) fn kotlin_android_dir(ctx: &ProjectContext) -> PathBuf {
    let app = ctx.script_dir.join("app");
    let compose = ctx.script_dir.join("composeApp");
    if app.is_dir() && !compose.is_dir() {
        app
    } else {
        compose
    }
}

pub fn detect() -> Result<ProjectContext> {
    let (script_dir, project_type) = detect_project_root()?;
    let android_home = detect_android_home();
    let emulator_path = detect_emulator_path(&android_home);
    let ndk_home = android_home
        .as_ref()
        .map(|a| a.join("ndk").join(platform::NDK_VERSION));
    let adb_path = android_home.as_ref().map(|a| {
        a.join("platform-tools")
            .join(platform::exe_name("adb"))
    });
    let sdk_manager = android_home.as_ref().map(|a| {
        a.join("cmdline-tools")
            .join("latest")
            .join("bin")
            .join(platform::exe_name("sdkmanager"))
    });
    let has_node_modules = script_dir.join("node_modules").exists();
    let has_android_project = match project_type {
        ProjectType::Tauri => platform::find_gradlew(
            &script_dir.join("src-tauri").join("gen").join("android"),
        )
        .is_some(),
        ProjectType::Flutter => platform::find_gradlew(&script_dir.join("android")).is_some(),
        ProjectType::Kotlin => platform::find_gradlew(&script_dir).is_some(),
        ProjectType::Rust => false,
    };
    let has_whatsapp_engine = project_type.whatsapp_path(&script_dir).exists();

    Ok(ProjectContext {
        script_dir,
        project_type,
        android_home,
        emulator_path,
        ndk_home,
        adb_path,
        sdk_manager,
        has_node_modules,
        has_android_project,
        has_whatsapp_engine,
    })
}

fn detect_project_root() -> Result<(PathBuf, ProjectType)> {
    let current = env::current_dir().context("No se pudo obtener el directorio actual")?;
    let mut dir = Some(current.as_path());

    while let Some(d) = dir {
        if d.join("pubspec.yaml").exists() {
            return Ok((d.to_path_buf(), ProjectType::Flutter));
        }
        if d.join("tauri.conf.json").exists() {
            return Ok((d.to_path_buf(), ProjectType::Tauri));
        }
        // Acepta `gradlew` o `gradlew.bat`: en Linux el wrapper no lleva extensión.
        if platform::find_gradlew(d).is_some() && d.join("settings.gradle.kts").exists() {
            return Ok((d.to_path_buf(), ProjectType::Kotlin));
        }
        if d.join("package.json").exists() {
            return Ok((d.to_path_buf(), ProjectType::Tauri));
        }
        if d.join("Cargo.toml").exists() {
            return Ok((d.to_path_buf(), ProjectType::Rust));
        }
        dir = d.parent();
    }

    anyhow::bail!(
        "No se encontró un proyecto Tauri, Flutter/Dart, Kotlin Multiplatform o Rust en este directorio ni en sus padres.\n\
         Asegúrate de ejecutar este comando dentro de la raíz de un proyecto válido."
    );
}

/// Detecta la ruta del emulador de Android desde ANDROID_HOME o rutas comunes.
fn detect_emulator_path(android_home: &Option<PathBuf>) -> Option<PathBuf> {
    let emu = platform::exe_name("emulator");
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(home) = android_home {
        candidates.push(home.join("emulator").join(&emu));
    }

    if platform::IS_WINDOWS {
        candidates.push(PathBuf::from(r"E:\SDKs\Android\emulator").join(&emu));
    } else {
        if let Some(h) = platform::home_dir() {
            candidates.push(h.join("Android").join("Sdk").join("emulator").join(&emu));
        }
        candidates.push(PathBuf::from("/usr/lib/android-sdk/emulator/emulator"));
    }

    candidates.into_iter().find(|c| c.exists())
}

fn detect_android_home() -> Option<PathBuf> {
    // 1. Variables de entorno (funcionan en ambas plataformas)
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(val) = env::var(key) {
            if val.is_empty() {
                continue;
            }
            let p = PathBuf::from(&val);
            if p.exists() {
                return Some(p);
            }
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    if platform::IS_WINDOWS {
        candidates.push(PathBuf::from(r"C:\Users\Admin\Android"));
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Android").join("Sdk"));
        }
        if let Some(up) = env::var_os("USERPROFILE") {
            candidates.push(
                PathBuf::from(up)
                    .join("AppData")
                    .join("Local")
                    .join("Android")
                    .join("Sdk"),
            );
        }
    } else {
        if let Some(h) = platform::home_dir() {
            candidates.push(h.join("Android").join("Sdk"));
            candidates.push(h.join("android-sdk"));
            candidates.push(h.join("Android").join("sdk"));
        }
        if let Some(d) = platform::data_dir() {
            candidates.push(d.join("Android").join("Sdk"));
        }
        candidates.push(PathBuf::from("/usr/lib/android-sdk"));
    }

    candidates.into_iter().find(|c| c.exists())
}