use std::path::PathBuf;
use std::env;
use anyhow::{Context, Result};

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
            Self::Tauri => root.join("src-tauri\\whatsapp-engine"),
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
            anyhow::bail!("[ERROR] ANDROID_HOME no está configurado.\nAsegúrate de tener instalado Android SDK en una de las rutas predeterminadas.");
        }
        Ok(())
    }
}

pub fn detect() -> Result<ProjectContext> {
    let (script_dir, project_type) = detect_project_root()?;
    let android_home = detect_android_home();
    let emulator_path = detect_emulator_path(&android_home);
    let ndk_home = android_home.as_ref().map(|a| a.join("ndk\\27.0.12077973"));
    let adb_path = android_home.as_ref().map(|a| a.join("platform-tools\\adb.exe"));
    let sdk_manager = android_home.as_ref().map(|a| a.join("cmdline-tools\\latest\\bin\\sdkmanager.bat"));
    let has_node_modules = script_dir.join("node_modules").exists();
    let has_android_project = match project_type {
        ProjectType::Tauri => script_dir.join("src-tauri\\gen\\android\\gradlew.bat").exists(),
        ProjectType::Flutter => script_dir.join("android\\gradlew.bat").exists(),
        ProjectType::Kotlin => script_dir.join("gradlew.bat").exists(),
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
        if d.join("gradlew.bat").exists() && d.join("settings.gradle.kts").exists() {
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

/// Detecta la ruta del emulator.exe desde ANDROID_HOME o desde rutas fijas.
fn detect_emulator_path(android_home: &Option<PathBuf>) -> Option<PathBuf> {
    let candidates = [
        android_home.as_ref().map(|h| h.join("emulator\\emulator.exe")),
        Some(PathBuf::from("E:\\SDKs\\Android\\emulator\\emulator.exe")),
    ];
    for c in candidates.iter().flatten() {
        if c.exists() {
            return Some(c.clone());
        }
    }
    None
}

fn detect_android_home() -> Option<PathBuf> {
    // 1. Check environment variable
    if let Ok(val) = env::var("ANDROID_HOME") {
        let p = PathBuf::from(val);
        if p.exists() { return Some(p); }
    }

    // 2. Check common locations
    let candidates = [
        "C:\\Users\\Admin\\Android",
        "C:\\Users\\Admin\\AppData\\Local\\Android\\Sdk",
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() { return Some(p); }
    }

    // 3. Check LOCALAPPDATA
    if let Ok(local) = env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local).join("Android\\Sdk");
        if p.exists() { return Some(p); }
    }

    // 4. Check USERPROFILE
    if let Ok(up) = env::var("USERPROFILE") {
        let p = PathBuf::from(up).join("AppData\\Local\\Android\\Sdk");
        if p.exists() { return Some(p); }
    }

    None
}
