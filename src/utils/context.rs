use std::path::PathBuf;
use std::env;
use anyhow::{Context, Result};

pub struct ProjectContext {
    pub script_dir: PathBuf,
    pub android_home: Option<PathBuf>,
    pub ndk_home: Option<PathBuf>,
    pub adb_path: Option<PathBuf>,
    pub sdk_manager: Option<PathBuf>,
    pub has_node_modules: bool,
    pub has_android_project: bool,
    pub has_tauri: bool,
}

impl ProjectContext {
    pub fn assert_android_sdk(&self) -> Result<()> {
        if self.android_home.is_none() {
            anyhow::bail!("[ERROR] ANDROID_HOME no está configurado.\nAsegúrate de tener instalado Android SDK en una de las rutas predeterminadas.");
        }
        Ok(())
    }

    pub fn assert_node_modules(&self) -> Result<()> {
        if !self.has_node_modules {
            anyhow::bail!("[INFO] node_modules no encontrado. Ejecutando npm install...");
        }
        Ok(())
    }
}

pub fn detect() -> Result<ProjectContext> {
    let script_dir = detect_project_root()?;
    let android_home = detect_android_home();
    let ndk_home = android_home.as_ref().map(|a| a.join("ndk\\27.0.12077973"));
    let adb_path = android_home.as_ref().map(|a| a.join("platform-tools\\adb.exe"));
    let sdk_manager = android_home.as_ref().map(|a| a.join("cmdline-tools\\latest\\bin\\sdkmanager.bat"));
    let has_node_modules = script_dir.join("node_modules").exists();
    let has_android_project = script_dir.join("src-tauri\\gen\\android\\gradlew.bat").exists();
    let has_tauri = script_dir.join("src-tauri\\tauri.conf.json").exists();

    Ok(ProjectContext {
        script_dir,
        android_home,
        ndk_home,
        adb_path,
        sdk_manager,
        has_node_modules,
        has_android_project,
        has_tauri,
    })
}

fn detect_project_root() -> Result<PathBuf> {
    let current = env::current_dir().context("No se pudo obtener el directorio actual")?;
    let mut dir = Some(current.as_path());

    while let Some(d) = dir {
        if d.join("tauri.conf.json").exists() || d.join("package.json").exists() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }

    anyhow::bail!(
        "No se encontró un proyecto Tauri/Node en este directorio ni en sus padres.\n\
         Asegúrate de ejecutar este comando dentro de la raíz de un proyecto Tauri."
    );
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
