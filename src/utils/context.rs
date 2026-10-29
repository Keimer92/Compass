use std::path::{Path, PathBuf};
use std::env;
use anyhow::{Context, Result};

use super::platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Tauri,
    Flutter,
    Kotlin,
    Python,
    Rust,
    /// Go + Wails v3 (`go.mod` con `wailsapp/wails` + `Taskfile.yml`).
    GoWails,
}

fn is_python_project(d: &Path) -> bool {
    let Ok(pyproject) = std::fs::read_to_string(d.join("pyproject.toml")) else {
        return false;
    };
    pyproject.lines().any(|line| {
        matches!(line.trim(), "[project]" | "[tool.poetry]")
    })
}

impl ProjectType {
    pub fn whatsapp_path(&self, root: &PathBuf) -> PathBuf {
        match self {
            Self::Tauri => root.join("src-tauri").join("whatsapp-engine"),
            Self::Flutter | Self::Kotlin | Self::Python | Self::Rust | Self::GoWails => {
                root.join("whatsapp_engine")
            }
        }
    }
}

pub struct ProjectContext {
    pub script_dir: PathBuf,
    pub project_type: ProjectType,
    pub android_home: Option<PathBuf>,
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
    let ndk_home = android_home
        .as_ref()
        .map(|a| a.join("ndk").join(platform::NDK_VERSION));
    let adb_path = android_home.as_ref().map(|a| {
        a.join("platform-tools")
            .join(platform::exe_name("adb"))
    });
    let sdk_manager = android_home.as_ref().and_then(|a| find_sdkmanager(a));
    let has_node_modules = script_dir.join("node_modules").exists();
    let has_android_project = match project_type {
        ProjectType::Tauri => platform::find_gradlew(
            &script_dir.join("src-tauri").join("gen").join("android"),
        )
        .is_some(),
        ProjectType::Flutter => platform::find_gradlew(&script_dir.join("android")).is_some(),
        ProjectType::Kotlin => platform::find_gradlew(&script_dir).is_some(),
        ProjectType::Python => false,
        ProjectType::Rust => false,
        // Wails v3 deja el proyecto Gradle en `build/android/`.
        ProjectType::GoWails => platform::find_gradlew(&script_dir.join("build").join("android"))
            .is_some(),
    };
    let has_whatsapp_engine = project_type.whatsapp_path(&script_dir).exists();

    Ok(ProjectContext {
        script_dir,
        project_type,
        android_home,
        ndk_home,
        adb_path,
        sdk_manager,
        has_node_modules,
        has_android_project,
        has_whatsapp_engine,
    })
}

/// ¿Este directorio (sin mirar hacia arriba) es la raíz de un proyecto?
///
/// Los proyectos Python se reconocen por metadatos de empaquetado en
/// `pyproject.toml`, no por la presencia aislada de cualquier fichero `.py`.
pub fn classify_dir(d: &Path) -> Option<ProjectType> {
    if d.join("pubspec.yaml").exists() {
        return Some(ProjectType::Flutter);
    }
    if d.join("tauri.conf.json").exists() {
        return Some(ProjectType::Tauri);
    }
    // Go + Wails v3: `go.mod` con la dependencia de Wails **y** el Taskfile
    // (v3 movió la configuración a `Taskfile.yml` + `build/config.yml`; la v2
    // usaba `wails.json` y otro CLI, así que no se acepta aquí).
    if is_wails_v3(d) {
        return Some(ProjectType::GoWails);
    }
    // Acepta `gradlew` o `gradlew.bat`: en Linux el wrapper no lleva extensión.
    if platform::find_gradlew(d).is_some() && d.join("settings.gradle.kts").exists() {
        return Some(ProjectType::Kotlin);
    }
    if d.join("package.json").exists() {
        return Some(ProjectType::Tauri);
    }
    if is_python_project(d) {
        return Some(ProjectType::Python);
    }
    if d.join("Cargo.toml").exists() {
        return Some(ProjectType::Rust);
    }
    None
}

/// ¿Es un proyecto Go + Wails **v3**?
///
/// Se exigen las dos señales: la dependencia `wailsapp/wails` en `go.mod` y el
/// `Taskfile.yml` (o `build/config.yml`), que es lo que usa la v3. La v2
/// (`wails.json`) tiene otro CLI (`wails`) y otro layout, así que no se mezcla.
fn is_wails_v3(d: &Path) -> bool {
    let Ok(go_mod) = std::fs::read_to_string(d.join("go.mod")) else {
        return false;
    };
    if !go_mod.contains("wailsapp/wails") {
        return false;
    }
    d.join("Taskfile.yml").exists() || d.join("build").join("config.yml").exists()
}

fn detect_project_root() -> Result<(PathBuf, ProjectType)> {
    let current = env::current_dir().context("No se pudo obtener el directorio actual")?;
    let mut dir = Some(current.as_path());

    while let Some(d) = dir {
        if let Some(pt) = classify_dir(d) {
            return Ok((d.to_path_buf(), pt));
        }
        dir = d.parent();
    }

    anyhow::bail!(
        "No se encontró un proyecto Tauri, Flutter/Dart, Kotlin Multiplatform, Python, Rust o Go/Wails en este directorio ni en sus padres.\n\
         Usa el selector con 'compass --pick' para elegir la carpeta del proyecto."
    );
}

/// Etiqueta corta del tipo de proyecto, para el selector.
fn type_label(pt: ProjectType) -> &'static str {
    match pt {
        ProjectType::Tauri => "Tauri",
        ProjectType::Flutter => "Flutter",
        ProjectType::Kotlin => "Kotlin",
        ProjectType::Python => "Python",
        ProjectType::Rust => "Rust",
        ProjectType::GoWails => "Go + Wails",
    }
}

/// Navegador interactivo para elegir la carpeta de un proyecto.
///
/// Hace falta porque desde el launcher de COSMIC el proceso arranca con el
/// directorio actual en `$HOME`, donde no hay ningún proyecto que detectar.
/// `compass` cae aquí cuando [`detect`] falla y no se pasó `--task`.
///
/// contract: devuelve la raíz elegida, o `Ok(None)` si el usuario cancela con Esc.
pub fn pick_project_root(start: &Path) -> Result<Option<PathBuf>> {
    use inquire::{Confirm, Select};

    let mut current = start
        .canonicalize()
        .unwrap_or_else(|_| start.to_path_buf());

    loop {
        let here = classify_dir(&current);
        // `options[i]` y `actions[i]` van siempre en paralelo: el índice elegido
        // en el prompt es el mismo que el de `actions`.
        let mut options: Vec<String> = Vec::new();
        let mut actions: Vec<Action> = Vec::new();

        // 1. Usar esta misma carpeta, si ya es un proyecto.
        if let Some(pt) = here {
            options.push(format!("✔  USAR ESTA CARPETA  ({})", type_label(pt)));
            actions.push(Action::Use);
        }

        // 2. Subir al padre.
        if let Some(parent) = current.parent() {
            if parent != current {
                options.push("↰  Subir al directorio padre".to_string());
                actions.push(Action::Up);
            }
        }

        // 3. Subdirectorios. Los ocultos van al final.
        let mut visible: Vec<PathBuf> = Vec::new();
        let mut hidden: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    hidden.push(p);
                } else {
                    visible.push(p);
                }
            }
        }
        visible.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        hidden.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));

        let subdirs: Vec<(PathBuf, Option<ProjectType>)> = visible
            .iter()
            .chain(hidden.iter())
            .map(|p| (p.clone(), classify_dir(p)))
            .collect();

        for (p, pt) in &subdirs {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            match pt {
                Some(t) => options.push(format!("  📂 {name}    ({})", type_label(*t))),
                None => options.push(format!("  📂 {name}")),
            }
            actions.push(match pt {
                Some(_) => Action::Enter(p.clone()),
                None => Action::Descend(p.clone()),
            });
        }

        if options.is_empty() {
            println!();
            println!("[INFO] No hay subdirectorios en: {}", current.display());
            if Confirm::new("¿Salir sin elegir proyecto?")
                .with_default(true)
                .prompt()?
            {
                return Ok(None);
            }
            continue;
        }

        // Muestra la ruta actual como cabecera del prompt para no perderse.
        let shown = current.display().to_string();
        let shown = if shown.len() > 96 {
            format!("…{}", &shown[shown.len() - 95..])
        } else {
            shown
        };
        println!();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║  SELECCIONA EL PROYECTO                          ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!("\n  📂 {shown}\n");

        // `raw_prompt()` devuelve el índice además del valor; `prompt()` sólo el
        // valor, y aquí necesitamos el índice para mapear a `actions`.
        let idx = match Select::new("Elige una carpeta:", options)
            .with_vim_mode(true)
            .with_page_size(14)
            .raw_prompt()
        {
            Ok(c) => c.index,
            // Esc / Ctrl-C = cancelar sin escribir nada.
            Err(_) => {
                println!("[INFO] Selección cancelada.");
                return Ok(None);
            }
        };

        match actions.get(idx).expect("índice dentro de rango") {
            Action::Use => return Ok(Some(current.clone())),
            Action::Up => {
                if let Some(parent) = current.parent() {
                    current = parent.to_path_buf();
                }
            }
            Action::Descend(p) | Action::Enter(p) => current = p.clone(),
        }
    }
}

enum Action {
    Use,
    Up,
    /// Entrar en un subdirectorio que no es proyecto, para seguir descendiendo.
    Descend(PathBuf),
    /// Entrar en un subdirectorio que ya es la raíz de un proyecto.
    Enter(PathBuf),
}

/// Localiza `sdkmanager` dentro de `$ANDROID_HOME/cmdline-tools`.
///
/// La documentación y casi todas las herramientas asumen el nombre estable
/// `cmdline-tools/latest`, pero el layout real varía:
///
/// - El instalador de Google descarga una carpeta versionada (`13.0`) y el
///   usuario (o el paquete de Ubuntu) crea `latest` como symlink.
/// - El paquete `google-android-cmdline-tools-*-installer` de Ubuntu **no**
///   crea ese symlink: deja `13.0` a secas, y sin él el runner nunca encontraba
///   `sdkmanager` y por tanto no podía instalar el NDK.
///
/// Se prueba `latest` primero (layout recomendado) y luego la carpeta
/// versionada más alta, ordenando numéricamente para que `9.0` no gane a
/// `13.0`.
fn find_sdkmanager(android_home: &Path) -> Option<PathBuf> {
    let bin_name = platform::exe_name("sdkmanager");
    let root = android_home.join("cmdline-tools");

    let latest = root.join("latest").join("bin").join(&bin_name);
    if latest.exists() {
        return Some(latest);
    }

    let mut versions: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let key: Vec<u64> = name.split('.').filter_map(|p| p.parse().ok()).collect();
            // Una carpeta "latest" residual, o sin dígitos, no es una versión.
            if key.is_empty() {
                return None;
            }
            let bin = e.path().join("bin").join(&bin_name);
            bin.exists().then_some((key, bin))
        })
        .collect();

    // Mayor versión primero.
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().next().map(|(_, bin)| bin)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_detection_requires_project_metadata() {
        let root = env::temp_dir().join(format!("compass-python-detect-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("pyproject.toml"), "[tool.ruff]\nline-length = 88\n").unwrap();
        assert_ne!(classify_dir(&root), Some(ProjectType::Python));

        std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"demo\"\n").unwrap();
        assert_eq!(classify_dir(&root), Some(ProjectType::Python));
        std::fs::write(root.join("pyproject.toml"), "[tool.poetry]\nname = \"demo\"\n").unwrap();
        assert_eq!(classify_dir(&root), Some(ProjectType::Python));
        let _ = std::fs::remove_dir_all(&root);
    }
}