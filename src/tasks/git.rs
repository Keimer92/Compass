//! Dominio único de Git: inicializar el repo, crear el `.gitignore` y conectar
//! el proyecto con un repositorio remoto (GitHub).
//!
//! Antes había dos tarjetas (`git-init` y `gitignore`) que son dos pasos del
//! mismo flujo: inicializas el repositorio y necesitas un `.gitignore` para no
//! subir `target/`, `node_modules/` ni artefactos de build. Ahora viven en una
//! sola tarjeta **Git**, junto con una opción nueva para enlazar el proyecto con
//! GitHub (vía URL o vía `gh repo create`).

use std::io::{self};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use inquire::Select;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame, Terminal,
};

use crate::theme::{self, Theme};
use crate::utils::{
    cmd,
    context::{ProjectContext, ProjectType},
    platform, ui,
};

// ───────────────────────────────────────────────────────────────────────────
// Helpers de ejecución
// ───────────────────────────────────────────────────────────────────────────

/// Ejecuta un programa en el proyecto y devuelve `Ok(())` o el texto del error
/// (stdout + stderr) para poder mostrarlo.
fn run_capture(
    program: &str,
    args: &[&str],
    dir: &Path,
) -> std::result::Result<(), String> {
    match Command::new(program).args(args).current_dir(dir).output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout);
            let err = String::from_utf8_lossy(&o.stderr);
            let joined = format!("{}\n{}", out.trim(), err.trim());
            Err(joined.trim().to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn git_capture(ctx: &ProjectContext, args: &[&str]) -> std::result::Result<(), String> {
    run_capture("git", args, &ctx.script_dir)
}

/// `stdout` recortado si `git <args>` tuvo éxito; `None` si falló.
fn git_out(ctx: &ProjectContext, args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .current_dir(&ctx.script_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// ¿Está `name` disponible como comando? (`gh`, por ejemplo.)
fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn is_repo(ctx: &ProjectContext) -> bool {
    ctx.script_dir.join(".git").exists()
}

fn has_commits(ctx: &ProjectContext) -> bool {
    git_out(ctx, &["rev-parse", "--verify", "HEAD"]).is_some()
}

// ───────────────────────────────────────────────────────────────────────────
// Inicialización
// ───────────────────────────────────────────────────────────────────────────

/// `git init` + rama `main` + primer commit. Idempotente.
///
/// Sin `ui::pause()`: la usan tanto la acción del menú como `connect_remote`,
/// que después continúa con el `remote add`. Devuelve `true` si al terminar hay
/// un repo con commits.
fn create_repo(ctx: &ProjectContext) -> Result<bool> {
    if is_repo(ctx) && has_commits(ctx) {
        return Ok(true);
    }

    if !is_repo(ctx) {
        println!("[INFO] Inicializando repositorio git...");
        if let Err(e) = git_capture(ctx, &["init"]) {
            eprintln!("[ERROR] git init falló: {e}");
            return Ok(false);
        }
        println!("[OK] Repositorio inicializado.");
    }

    // Rama por defecto a `main` (git puede haber creado `master`).
    let branch =
        git_out(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "main".into());
    if branch != "main" {
        println!("[INFO] Renombrando rama a 'main'...");
        if git_capture(ctx, &["branch", "-M", "main"]).is_ok() {
            println!("[OK] Rama renombrada a 'main'.");
        } else {
            eprintln!("[WARN] No se pudo renombrar la rama a 'main'.");
        }
    } else {
        println!("[OK] La rama ya se llama 'main'.");
    }

    println!("[INFO] Agregando archivos al staging...");
    let _ = git_capture(ctx, &["add", "."]);
    println!("[OK] Archivos agregados.");

    println!("[INFO] Creando primer commit...");
    match git_capture(ctx, &["commit", "-m", "First Commit"]) {
        Ok(()) => println!("[OK] Primer commit creado: 'First Commit'."),
        Err(e)
            if e.contains("nothing to commit")
                || e.contains("no changes")
                || e.contains("nothing added") =>
        {
            println!("[INFO] No había cambios que commitear.");
        }
        Err(e) => {
            eprintln!("[WARN] git commit: {e}");
            if e.contains("who you are") || e.contains("user.email") || e.contains("user.name") {
                println!("       Configura tu identidad de git:");
                println!("         git config --global user.name \"Tu Nombre\"");
                println!("         git config --global user.email \"tu@email.com\"");
            }
        }
    }

    Ok(has_commits(ctx))
}

fn init(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner("INICIALIZAR REPOSITORIO GIT (main)");
    println!();

    if is_repo(ctx) && has_commits(ctx) {
        println!("[INFO] El repositorio ya existe y tiene commits.");
        println!("  No se realizó ningún cambio.");
        ui::pause();
        return Ok(());
    }

    if !create_repo(ctx)? {
        println!("[WARN] El repositorio quedó sin commits; revisa la configuración de git.");
    }
    println!();
    ui::pause();
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// Conexión con un repositorio remoto (GitHub)
// ───────────────────────────────────────────────────────────────────────────

/// ¿Parece una URL de repositorio git? (Se avisa, pero no se bloquea.)
fn looks_like_git_url(url: &str) -> bool {
    let u = url.trim();
    let scheme_ok = u.starts_with("https://")
        || u.starts_with("http://")
        || u.starts_with("git@")
        || u.starts_with("ssh://");
    scheme_ok && (u.contains('/') || u.contains(':'))
}

/// URL navegable equivalente a un remoto (`git@host:path` o `https://…`).
fn web_url(remote: &str) -> Option<String> {
    let r = remote.trim();
    if let Some(rest) = r.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return Some(format!(
                "https://{}/{}",
                host,
                path.trim_end_matches(".git")
            ));
        }
    }
    if r.starts_with("http://") || r.starts_with("https://") {
        return Some(r.trim_end_matches(".git").to_string());
    }
    None
}

fn push_current_branch(ctx: &ProjectContext) -> Result<()> {
    let mut branch =
        git_out(ctx, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "main".into());
    if branch == "HEAD" || branch.is_empty() {
        branch = "main".to_string();
    }

    println!(
        "[INFO] Subiendo la rama '{}' (git push -u origin {})...",
        branch, branch
    );
    match git_capture(ctx, &["push", "-u", "origin", branch.as_str()]) {
        Ok(()) => {
            println!("[OK] Rama subida. 'origin/{}' queda como upstream.", branch);
            if let Some(web) = git_out(ctx, &["remote", "get-url", "origin"]).and_then(|u| web_url(&u)) {
                println!("     Repositorio: {}", web);
            }
        }
        Err(e) => {
            eprintln!("[ERROR] git push falló:\n{}", e);
            println!();
            println!("Pistas:");
            println!("  - ¿Estás autenticado? (Personal Access Token en HTTPS, o `gh auth login`).");
            println!("  - Si el remoto ya tenía contenido: git pull --rebase origin {branch}");
            println!("  - El repo debe existir en GitHub y tener permiso de escritura.");
        }
    }
    Ok(())
}

/// Crea el repositorio en GitHub con `gh` y sube el código en un solo paso.
fn create_github_repo(ctx: &ProjectContext) -> Result<()> {
    println!();
    if !command_exists("gh") {
        println!("[WARN] `gh` (GitHub CLI) no está instalado.");
        println!("       Usa la opción 1 y pega la URL, o instala gh: https://cli.github.com");
        ui::pause();
        return Ok(());
    }
    if git_out(ctx, &["remote", "get-url", "origin"]).is_some() {
        println!("[WARN] Ya existe un remoto 'origin'.");
        println!("       Bórralo primero si quieres recrearlo: git remote remove origin");
        ui::pause();
        return Ok(());
    }

    let default_name = ctx
        .script_dir
        .file_name()
        .map(|n| n.to_string_lossy().replace(' ', "-"))
        .unwrap_or_else(|| "mi-proyecto".into());
    let name = ui::ask_text("Nombre del repositorio en GitHub", Some(default_name.as_str()))?;
    if name.trim().is_empty() {
        println!("[INFO] Operación cancelada.");
        ui::pause();
        return Ok(());
    }

    let visibility = Select::new("Visibilidad:", vec!["Privado", "Público"])
        .with_vim_mode(true)
        .prompt()?;
    let vis_flag = if visibility == "Público" { "--public" } else { "--private" };

    println!();
    println!("[INFO] Creando el repositorio y subiendo el código (gh repo create)...");
    let args = [
        "repo",
        "create",
        name.as_str(),
        "--source=.",
        "--remote=origin",
        vis_flag,
        "--push",
    ];
    match run_capture("gh", &args, &ctx.script_dir) {
        Ok(()) => {
            println!("[OK] Repositorio '{}' creado y código subido.", name);
        }
        Err(e) => {
            eprintln!("[ERROR] gh repo create falló:\n{}", e);
            println!();
            println!("Pistas:");
            println!("  - ¿Sesión iniciada? Ejecuta: gh auth login");
            println!("  - ¿El nombre ya existe en tu cuenta? Prueba otro.");
        }
    }
    ui::pause();
    Ok(())
}

fn connect_remote(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner("CONECTAR CON REPOSITORIO REMOTO");
    println!();

    // 1. Necesitamos un repo local con commits para poder subir algo.
    if !is_repo(ctx) || !has_commits(ctx) {
        println!("[INFO] Este proyecto todavía no es un repositorio git con commits.");
        let init_now = inquire::Confirm::new("¿Inicializarlo ahora (git init + primer commit)?")
            .with_default(true)
            .prompt()?;
        if !init_now {
            println!("[INFO] Operación cancelada.");
            ui::pause();
            return Ok(());
        }
        if !create_repo(ctx)? {
            println!("[WARN] No se pudo preparar el repositorio local.");
            ui::pause();
            return Ok(());
        }
        println!();
    }

    // 2. Estado actual del remoto.
    let existing = git_out(ctx, &["remote", "get-url", "origin"]);
    if let Some(url) = &existing {
        println!("[INFO] Ya hay un remoto 'origin': {url}");
    }

    // 3. ¿URL existente o repo nuevo con gh?
    let mut modes = vec!["1. Ya tengo el repositorio en GitHub (pego la URL)"];
    if command_exists("gh") {
        modes.push("2. Crear un repositorio nuevo en GitHub (gh)");
    }
    let mode = Select::new("¿Cómo quieres conectar?", modes)
        .with_vim_mode(true)
        .prompt()?;

    if mode.starts_with("2.") {
        return create_github_repo(ctx);
    }

    // 4. URL del remoto.
    let url = ui::ask_text(
        "URL del repositorio (https://github.com/usuario/repo.git o git@github.com:usuario/repo.git)",
        existing.as_deref(),
    )?;
    if url.trim().is_empty() {
        println!("[INFO] No especificaste una URL.");
        ui::pause();
        return Ok(());
    }
    if !looks_like_git_url(&url) {
        println!("[WARN] La URL no parece de un repositorio git; se usará tal cual.");
    }

    // 5. Alta o actualización del remoto `origin`.
    match &existing {
        Some(old) if *old == url => {
            println!("[INFO] El remoto 'origin' ya apunta a esa URL.");
        }
        Some(_) => {
            println!("[INFO] Reemplazando la URL del remoto 'origin'...");
            if let Err(e) = git_capture(ctx, &["remote", "set-url", "origin", url.as_str()]) {
                eprintln!("[ERROR] No se pudo actualizar el remoto: {e}");
                ui::pause();
                return Ok(());
            }
            println!("[OK] Remoto 'origin' actualizado.");
        }
        None => {
            println!("[INFO] Agregando remoto 'origin'...");
            if let Err(e) = git_capture(ctx, &["remote", "add", "origin", url.as_str()]) {
                eprintln!("[ERROR] No se pudo agregar el remoto: {e}");
                ui::pause();
                return Ok(());
            }
            println!("[OK] Remoto 'origin' agregado.");
        }
    }

    // 6. Push inicial de la rama actual.
    println!();
    push_current_branch(ctx)?;

    println!();
    ui::pause();
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// Estado
// ───────────────────────────────────────────────────────────────────────────

fn show_status(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner("ESTADO DEL REPOSITORIO");
    println!();

    if !is_repo(ctx) {
        println!("[INFO] No hay repositorio git en este proyecto.");
        ui::pause();
        return Ok(());
    }

    let branch = git_out(ctx, &["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "(sin commits)".into());
    println!("  Rama:    {}", branch);

    match git_out(ctx, &["remote", "get-url", "origin"]) {
        Some(u) => println!("  Remoto:  {}", u),
        None => println!("  Remoto:  (ninguno)"),
    }

    let commits = git_out(ctx, &["rev-list", "--count", "HEAD"]).unwrap_or_else(|| "0".into());
    println!("  Commits: {}", commits);
    println!();
    println!("Cambios sin commitear:");

    let status = git_out(ctx, &["status", "--short"]).unwrap_or_default();
    if status.trim().is_empty() {
        println!("  (árbol de trabajo limpio)");
    } else {
        for line in status.lines() {
            println!("  {}", line);
        }
    }

    println!();
    ui::pause();
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// .gitignore
// ───────────────────────────────────────────────────────────────────────────

fn gitignore_content(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Rust => r#"# ============================================
# .gitignore para Rust / Cargo
# ============================================

# --- Build artifacts (el mas pesado) ---
target/

# --- Cargo lock (normalmente se incluye en binarios) ---
# Cargo.lock

# --- IDE ---
.vscode/
.idea/
*.iml
*.sublime-project
*.sublime-workspace

# --- OS ---
.DS_Store
Thumbs.db
Desktop.ini
*.swp
*.swo
*~

# --- Environment ---
.env
.env.example
.env.local
.env.*.local

# --- Logs ---
*.log
"#,
        ProjectType::Tauri => r#"# ============================================
# .gitignore para Rust/Tauri + Windows/Android
# ============================================

# --- Rust / Cargo (build artifacts, muy pesados) ---
src-tauri/target/

# --- Node.js ---
node_modules/

# --- Vite cache ---
node_modules/.vite

# --- Frontend build output ---
dist/
build/

# --- Android nativo (auto-generado por 'npx tauri android init') ---
src-tauri/gen/android/app/build/
src-tauri/gen/android/.gradle/
src-tauri/gen/android/build/
src-tauri/gen/android/gradle/
src-tauri/gen/android/buildSrc/
src-tauri/gen/android/app/keimer
src-tauri/gen/android/app/keimer.keystore
src-tauri/gen/android/app/keystore.properties
src-tauri/gen/android/app/proguard-rules.pro
src-tauri/gen/android/app/proguard-tauri.pro
src-tauri/gen/android/app/tauri.build.gradle.kts
src-tauri/gen/android/app/tauri.properties
src-tauri/gen/android/app/src/main/assets/
src-tauri/gen/android/app/src/main/jniLibs/
src-tauri/gen/android/app/src/main/java/com/schooltracker/proela/generated/
src-tauri/gen/schemas/

# --- Sistema operativo ---
.DS_Store
Thumbs.db
Desktop.ini
*.swp
*.swo
*~

# --- IDE ---
.vscode/
.idea/
*.sublime-project
*.sublime-workspace

# --- Variables de entorno ---
.env
.env.example
.env.local
.env.*.local

# --- Logs ---
*.log
"#,
        ProjectType::Flutter => r#"# ============================================
# .gitignore para Flutter/Dart
# ============================================

# --- Flutter/Dart ---
build/
.dart_tool/
.packages
.flutter-plugins
.flutter-plugins-dependencies

# --- Android (Gradle) ---
android/build/
android/.gradle/
android/app/build/
android/local.properties
android/key.properties

# --- Node.js ---
node_modules/

# --- ios ---
ios/.symlinks/
ios/Pods/
ios/Runner.xcworkspace/
ios/Flutter/Flutter.framework
ios/Flutter/Flutter.podspec

# --- Sistema operativo ---
.DS_Store
Thumbs.db
Desktop.ini
*.swp
*.swo
*~

# --- IDE ---
.vscode/
.idea/
*.sublime-project
*.sublime-workspace

# --- Variables de entorno ---
.env
.env.example
.env.local
.env.*.local

# --- Logs ---
*.log
"#,
        ProjectType::Kotlin => r#"# ============================================
# .gitignore para Kotlin Multiplatform
# ============================================

# --- Gradle Build Artifacts ---
build/
.gradle/
local.properties
*.apk
*.aab
*.ap_
*.dex
composeApp/build/
composeApp/out/
composeApp/release/

# --- Kotlin ---
.kotlin/

# --- Signing / Secrets ---
composeApp/keystore.properties

# --- IDE ---
.idea/
*.iml
.vscode/
*.sublime-project
*.sublime-workspace

# --- OS ---
.DS_Store
Thumbs.db
Desktop.ini
*.swp
*.swo
*~

# --- Environment ---
.env
.env.example
.env.local
.env.*.local

# --- Logs ---
*.log
"#,
        ProjectType::GoWails => r#"# ============================================
# .gitignore para Go + Wails v3
# ============================================

# --- Binarios generados ---
bin/

# --- Firma de Android (keystore + contraseñas: NO subir) ---
android-signing/
keystore-backups/

# --- Node.js / frontend ---
frontend/node_modules/
frontend/dist/
frontend/bindings/

# --- Android (generado por 'wails3 update build-assets') ---
build/android/app/build/
build/android/.gradle/
build/android/app/src/main/jniLibs/
build/android/app/src/main/assets/
build/android/overlay.json
build/android/local.properties
build/android/gen/
*.apk
*.aab

# --- Go ---
*.syso
*.test

# --- IDE ---
.idea/
*.iml
.vscode/
*.sublime-project
*.sublime-workspace

# --- OS ---
.DS_Store
Thumbs.db
Desktop.ini
*.swp
*.swo
*~

# --- Environment ---
.env
.env.example
.env.local
.env.*.local

# --- Logs ---
*.log
"#,
        ProjectType::Python => r#"# ============================================
# .gitignore para Python
# ============================================

# --- Python bytecode y caches ---
__pycache__/
*.py[cod]
*$py.class
.pytest_cache/
.mypy_cache/
.ruff_cache/
.coverage
htmlcov/

# --- Entornos virtuales ---
.venv/
venv/
ENV/

# --- Build / packaging ---
build/
dist/
*.egg-info/

# --- Secrets / local config ---
.env
.env.*
!.env.example

# --- IDE / OS / logs ---
.vscode/
.idea/
.DS_Store
Thumbs.db
*.log
"#,
    }
}

/// Adds Windows-specific rules without removing existing project rules.
fn gitignore_content_for_platform(project_type: ProjectType) -> String {
    let mut content = gitignore_content(project_type).trim_end().to_string();
    content.push_str("\n\n# --- Windows ---\n$RECYCLE.BIN/\nehthumbs.db\n*.lnk\n");
    content
}

/// Keep all existing content intact and append only template rules that are
/// absent as exact ignore patterns.
fn merge_gitignore(existing: &str, required: &str) -> String {
    let present: std::collections::HashSet<&str> = existing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let missing: Vec<&str> = required
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !present.contains(line))
        .collect();

    if missing.is_empty() {
        return existing.to_string();
    }

    let mut merged = existing.to_string();
    if !merged.is_empty() && !merged.ends_with('\n') {
        merged.push('\n');
    }
    if !merged.is_empty() {
        merged.push_str("\n# Compass project ignore rules\n");
    }
    for rule in missing {
        merged.push_str(rule);
        merged.push('\n');
    }
    merged
}

/// Non-interactive .gitignore action used by file-manager context menus.
pub fn add_context_gitignore(ctx: &ProjectContext) -> Result<()> {
    let path = ctx.script_dir.join(".gitignore");
    let required = gitignore_content_for_platform(ctx.project_type);
    let existing = if path.exists() {
        Some(
            std::fs::read_to_string(&path)
                .with_context(|| format!("No se pudo leer {}", path.display()))?,
        )
    } else {
        None
    };
    let content = match &existing {
        Some(existing) => merge_gitignore(existing, &required),
        None => required,
    };
    if existing.as_deref() == Some(content.as_str()) {
        println!("[OK] .gitignore ya contiene las reglas necesarias: {}", path.display());
        return Ok(());
    }

    std::fs::write(&path, content)
        .with_context(|| format!("No se pudo escribir {}", path.display()))?;
    println!("[OK] .gitignore actualizado: {}", path.display());
    Ok(())
}

fn print_preserved(project_type: ProjectType) {
    match project_type {
        ProjectType::Rust => {
            println!("  - src/ (código Rust)");
            println!("  - Cargo.toml + Cargo.lock");
            println!("  - build.rs (si existe)");
            println!("  - tests/ (pruebas de integracion)");
            println!("  - examples/ (ejemplos)");
            println!("  - assets/ (recursos estaticos)");
        }
        ProjectType::Tauri => {
            println!("  - src/ (código frontend)");
            println!("  - src-tauri/src/ (código Rust)");
            println!("  - src-tauri/Cargo.toml + Cargo.lock");
            println!("  - src-tauri/tauri.conf.json");
            println!("  - src-tauri/icons/");
            println!("  - src-tauri/capabilities/");
            println!("  - src-tauri/build.rs");
            println!("  - package.json + package-lock.json");
            println!("  - tsconfig.json, vite.config.ts");
            println!("  - index.html, public/");
        }
        ProjectType::Flutter => {
            println!("  - lib/ (código Dart)");
            println!("  - test/ (pruebas)");
            println!("  - pubspec.yaml + pubspec.lock");
            println!("  - android/ (configuración)");
            println!("  - ios/ (configuración)");
            println!("  - web/ (configuración)");
        }
        ProjectType::Kotlin => {
            println!("  - composeApp/src/ (código Kotlin)");
            println!("  - composeApp/build.gradle.kts");
            println!("  - settings.gradle.kts, gradle.properties");
            println!("  - gradle/wrapper/ (Gradle Wrapper)");
            println!("  - gradlew + gradlew.bat (wrappers de ambas plataformas)");
        }
        ProjectType::GoWails => {
            println!("  - main.go, internal/ (código Go)");
            println!("  - go.mod + go.sum");
            println!("  - Taskfile.yml + build/ (assets de build de Wails)");
            println!("  - frontend/src/ (código del frontend)");
            println!("  - frontend/package.json + package-lock.json");
        }
        ProjectType::Python => {
            println!("  - pyproject.toml");
            println!("  - código Python y pruebas");
            println!("  - requirements*.txt / lockfiles si existen");
        }
    }
}

fn print_instructions(project_type: ProjectType) {
    match project_type {
        ProjectType::Rust => {
            println!("  1. git clone <repo>");
            println!("  2. cargo build");
            println!("  3. cargo run");
        }
        ProjectType::Tauri => {
            println!("  1. git clone <repo>");
            println!("  2. npm install");
            println!("  3. npx tauri android init (solo si compilas para Android)");
            println!("  4. npm run tauri dev (Desktop) / tauri android dev (Android)");
        }
        ProjectType::Flutter => {
            println!("  1. git clone <repo>");
            println!("  2. flutter pub get");
            println!("  3. flutter run (Desktop) / flutter run (Android)");
        }
        ProjectType::Kotlin => {
            println!("  1. git clone <repo>");
            println!("  2. ./gradlew build");
            println!("  3. ./gradlew :composeApp:run (Desktop)");
            println!("  4. ./gradlew :composeApp:installDebug (Android)");
        }
        ProjectType::GoWails => {
            println!("  1. git clone <repo>");
            println!("  2. go install github.com/wailsapp/wails/v3/cmd/wails3@latest");
            println!("  3. npm install  (en frontend/)");
            println!("  4. wails3 task dev  (Desktop) / wails3 task build");
        }
        ProjectType::Python => {
            println!("  1. git clone <repo>");
            println!("  2. python -m venv .venv");
            println!("  3. instala las dependencias del proyecto");
            println!("  4. ejecuta el entry point del proyecto");
        }
    }
}

fn gitignore_banner(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Rust => "CREAR .gitignore PARA RUST",
        ProjectType::Tauri => "CREAR .gitignore PARA RUST/TAURI",
        ProjectType::Flutter => "CREAR .gitignore PARA FLUTTER/DART",
        ProjectType::Kotlin => "CREAR .gitignore PARA KOTLIN MULTIPLATFORM",
        ProjectType::GoWails => "CREAR .gitignore PARA GO + WAILS",
        ProjectType::Python => "CREAR .gitignore PARA PYTHON",
    }
}

fn write_gitignore(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner(gitignore_banner(ctx.project_type));
    println!();
    println!("Se ignorarán archivos de compilación, dependencias, caches");
    println!("y archivos del sistema/IDE.");
    println!("Se conservará código fuente, configuraciones y assets.");
    println!();

    let confirm = inquire::Confirm::new("¿Crear/sobrescribir .gitignore?")
        .with_default(false)
        .prompt()?;

    if !confirm {
        println!("[INFO] Operación cancelada.");
        ui::pause();
        return Ok(());
    }

    std::fs::write(ctx.script_dir.join(".gitignore"), gitignore_content(ctx.project_type))?;

    println!();
    println!("[OK] .gitignore creado exitosamente!");
    println!();
    println!("Archivos PRESERVADOS en git (esenciales para compilar):");
    print_preserved(ctx.project_type);
    println!();
    println!("Para compilar en máquina nueva:");
    print_instructions(ctx.project_type);
    println!();
    ui::pause();
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// Card
// ───────────────────────────────────────────────────────────────────────────

/// Menú unificado de Git (absorbe las antiguas tarjetas Git Init y .gitignore).
pub fn manage(ctx: &ProjectContext) -> Result<()> {
    let theme = theme::load();
    let mut state = GitState::new(*theme);
    state.trace = TRACE.load(Ordering::Relaxed);

    let mut terminal = setup_git_terminal()?;
    cmd::set_console_title(&platform::dir_display_name(&ctx.script_dir));

    let result = (|| -> anyhow::Result<GitAction> {
        loop {
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            terminal.draw(|f| draw_git_menu(f, &mut state, ctx))?;

            match handle_git_event(&mut state, area)? {
                GitAction::Quit => return Ok(GitAction::Quit),
                GitAction::Continue => {}
                GitAction::Run(item) => {
                    if item == GitItem::Back {
                        return Ok(GitAction::Run(item));
                    }
                    restore_git_terminal(&mut terminal)?;
                    let action_result = run_git_action(item, ctx);
                    let new_terminal = setup_git_terminal()?;
                    terminal = new_terminal;
                    cmd::set_console_title(&platform::dir_display_name(&ctx.script_dir));
                    action_result?;
                }
            }
        }
    })();

    let _ = restore_git_terminal(&mut terminal);
    match result? {
        GitAction::Quit | GitAction::Run(GitItem::Back) => Ok(()),
        _ => unreachable!("manage solo retorna en Quit o Back"),
    }
}

/// TUI de cards para el menú de Git (mismo estilo que el menú principal)
static TRACE: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
fn trace_event(msg: &str) {
    if !TRACE.load(Ordering::Relaxed) {
        return;
    }
    let log = std::env::temp_dir().join("compass-git-tui.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        use std::io::Write;
        let _ = writeln!(f, "{msg}");
    }
}

const GIT_COLS: usize = 5;
const GIT_CARD_W: u16 = 26;
const GIT_CARD_H: u16 = 5;
const GIT_FOOTER_H: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitItem {
    Init,
    Gitignore,
    ConnectRemote,
    Status,
    Back,
}

impl GitItem {
    fn icon(&self) -> &str {
        match self {
            Self::Init => "\u{1f4c1}",
            Self::Gitignore => "\u{1f6ab}",
            Self::ConnectRemote => "\u{1f517}",
            Self::Status => "\u{1f4ca}",
            Self::Back => "\u{21a9}",
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Init => "Inicializar repositorio",
            Self::Gitignore => "Crear/actualizar .gitignore",
            Self::ConnectRemote => "Conectar con remoto",
            Self::Status => "Ver estado",
            Self::Back => "Volver al menu principal",
        }
    }

    fn description(&self) -> &str {
        match self {
            Self::Init => "git init + main + primer commit",
            Self::Gitignore => "Plantilla por tipo de proyecto",
            Self::ConnectRemote => "Enlazar con GitHub (URL o gh)",
            Self::Status => "Rama, remoto y cambios",
            Self::Back => "Regresa al menu principal",
        }
    }

    fn section(&self) -> &str {
        "MNT"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitAction {
    Run(GitItem),
    Quit,
    Continue,
}

#[derive(Debug, Clone)]
pub struct GitState {
    pub items: Vec<GitItem>,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub theme: Theme,
    pub trace: bool,
}

impl GitState {
    pub fn new(theme: Theme) -> Self {
        let items = vec![
            GitItem::Init,
            GitItem::Gitignore,
            GitItem::ConnectRemote,
            GitItem::Status,
            GitItem::Back,
        ];
        Self {
            items,
            selected: 0,
            hovered: None,
            theme,
            trace: false,
        }
    }

    fn prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn next(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct GroupSize {
    cards: usize,
    rows: usize,
    height: u16,
}

fn calc_git_group_sizes(cards: &[GitItem]) -> Vec<GroupSize> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < cards.len() {
        let mut count = 0;
        while start + count < cards.len() && count < GIT_COLS {
            count += 1;
        }
        let rows = (count as u16 + GIT_COLS as u16 - 1) / GIT_COLS as u16;
        let height = rows * GIT_CARD_H + 1;
        groups.push(GroupSize { cards: count, rows: rows as usize, height });
        start += count;
    }
    groups
}

fn calc_git_total_height(cards: &[GitItem]) -> u16 {
    let groups = calc_git_group_sizes(cards);
    groups.iter().map(|g| g.height).sum::<u16>() + (groups.len().saturating_sub(1) as u16)
}

fn create_git_card(
    item: GitItem,
    is_selected: bool,
    is_hovered: bool,
    theme: &Theme,
) -> Paragraph<'static> {
    let section_color = theme.section(item.section());
    let icon = item.icon();
    let label = item.label();
    let desc = item.description();

    let mut lines: Vec<Line<'static>> = Vec::new();

    let mut line1_spans: Vec<Span<'static>> = vec![Span::raw(format!(" {icon}  "))];
    if is_selected {
        line1_spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(section_color).add_modifier(Modifier::BOLD),
        ));
    } else if is_hovered {
        line1_spans.push(Span::styled(label.to_string(), Style::default().fg(theme.text)));
    } else {
        line1_spans.push(Span::styled(label.to_string(), Style::default().fg(theme.muted)));
    }
    lines.push(Line::from(line1_spans));

    lines.push(Line::from(Span::styled(
        format!("   {desc}"),
        Style::default().fg(theme.muted),
    )));

    lines.push(Line::from(Span::styled(
        " ".repeat(GIT_CARD_W as usize - 2),
        Style::default().fg(theme.border),
    )));

    let indicator = if is_selected {
        ">>> SELECCIONADO <<<"
    } else if is_hovered {
        "--- hover ---"
    } else {
        ""
    };
    if !indicator.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {indicator}"),
            Style::default().fg(section_color).add_modifier(Modifier::BOLD),
        )));
    }

    let border_style = if is_selected {
        Style::default().fg(section_color).add_modifier(Modifier::BOLD)
    } else if is_hovered {
        Style::default().fg(theme.text)
    } else {
        Style::default().fg(theme.border)
    };

    let bg = if is_selected {
        Some(theme.selected_bg(item.section()))
    } else if is_hovered {
        Some(theme.hover_bg)
    } else {
        None
    };

    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .style(Style::default().bg(bg.unwrap_or(Color::Reset))),
        )
        .alignment(Alignment::Left)
}

fn git_card_at(cards: &[GitItem], x: u16, y: u16, area: Rect) -> Option<usize> {
    let groups = calc_git_group_sizes(cards);
    let total_h = calc_git_total_height(cards);

    let content_width = area.width as usize;
    let content_height = area.height as usize;

    let card_w = GIT_CARD_W as usize;
    let card_h = GIT_CARD_H as usize;
    let cols = GIT_COLS;

    let total_card_w = cols * card_w;
    let total_card_h = total_h as usize;

    let offset_x = content_width.saturating_sub(total_card_w) / 2;
    let offset_y = content_height.saturating_sub(total_card_h) / 2;

    if x < offset_x as u16 || x >= (offset_x + total_card_w) as u16 {
        return None;
    }
    if y < offset_y as u16 || y >= (offset_y + total_card_h) as u16 {
        return None;
    }

    let rel_x = (x as usize) - offset_x;
    let rel_y = (y as usize) - offset_y;

    let col = rel_x / card_w;

    let mut card_index = 0;
    let mut y_acc = 0;
    for group in &groups {
        if rel_y < y_acc + group.height as usize {
            let row_in_group_adj = rel_y - y_acc;
            let card_row = row_in_group_adj / card_h as usize;
            if card_row < group.rows as usize {
                let idx = card_row * cols + col;
                if idx < group.cards {
                    return Some(card_index + idx);
                }
            }
            return None;
        }
        y_acc += group.height as usize;
        card_index += group.cards;
    }

    None
}

fn render_git_cards(
    frame: &mut Frame,
    area: Rect,
    cards: &[GitItem],
    state: &GitState,
) {
    let groups = calc_git_group_sizes(cards);
    let total_h = calc_git_total_height(cards);

    let content_width = area.width as usize;
    let content_height = area.height as usize;

    let card_w = GIT_CARD_W as usize;
    let card_h = GIT_CARD_H as usize;
    let cols = GIT_COLS;

    let total_card_w = cols * card_w;
    let total_card_h = total_h as usize;

    let offset_x = content_width.saturating_sub(total_card_w) / 2;
    let offset_y = content_height.saturating_sub(total_card_h) / 2;

    let mut card_index = 0;
    let mut y_acc = 0;
    for group in &groups {
        let x_start = offset_x as u16;
        let y_start = (offset_y as u16).saturating_add(y_acc as u16);

        for i in 0..group.cards {
            let col = i % cols;
            let row = i / cols;
            let card_x = x_start.saturating_add((col * card_w) as u16);
            let card_y = y_start.saturating_add((row * card_h) as u16);
            let card_area = Rect::new(card_x, card_y, card_w as u16, card_h as u16);

            let item = cards[card_index];
            let is_selected = card_index == state.selected;
            let is_hovered = state.hovered.map_or(false, |h| h == card_index);

            let card = create_git_card(item, is_selected, is_hovered, &state.theme);
            frame.render_widget(card, card_area);
            card_index += 1;
        }
        y_acc += group.height as usize;
    }
}

fn setup_git_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_git_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

fn git_cards_area(area: Rect) -> Rect {
    Rect::new(
        area.left(),
        area.top() + 6,
        area.width,
        area.height.saturating_sub(6 + GIT_FOOTER_H),
    )
}

fn draw_git_menu(frame: &mut Frame, state: &mut GitState, ctx: &ProjectContext) {
    let area = frame.area();
    let theme = state.theme;

    let background = Block::default().style(Style::default().bg(theme.background));
    frame.render_widget(background, area);

    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "  GIT  ",
            Style::default().fg(theme.brand).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("  Proyecto: {}", ctx.script_dir.display()),
            Style::default().fg(theme.muted),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );
    let title_area = Rect::new(area.left() + 2, area.top() + 1, area.width.saturating_sub(4), 5);
    frame.render_widget(title, title_area);

    let cards_area = git_cards_area(area);
    render_git_cards(frame, cards_area, &state.items, state);

    let footer_text = "  Flechas / WASD: navegar  Enter: ejecutar  Esc: volver";
    let footer = Paragraph::new(Span::styled(
        footer_text,
        Style::default().fg(theme.muted),
    ));
    let footer_area = Rect::new(
        area.left() + 2,
        area.top() + area.height.saturating_sub(GIT_FOOTER_H + 1),
        area.width.saturating_sub(4),
        GIT_FOOTER_H,
    );
    frame.render_widget(footer, footer_area);
}

fn handle_git_event(
    state: &mut GitState,
    area: Rect,
) -> anyhow::Result<GitAction> {
    if crossterm::event::poll(std::time::Duration::from_millis(50))? {
        let event = crossterm::event::read()?;
        return Ok(handle_git_input(state, area, event));
    }
    Ok(GitAction::Continue)
}

fn handle_git_input(
    state: &mut GitState,
    area: Rect,
    event: Event,
) -> GitAction {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => GitAction::Quit,
            KeyCode::Up | KeyCode::Char('k' | 'K') | KeyCode::Left | KeyCode::Char('h' | 'H') => {
                state.prev();
                GitAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') | KeyCode::Right | KeyCode::Char('l' | 'L') => {
                state.next();
                GitAction::Continue
            }
            KeyCode::Enter => GitAction::Run(state.items[state.selected]),
            _ => GitAction::Continue,
        },
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::Up(MouseButton::Left) => {
                    if let Some(idx) = git_card_at(
                        &state.items,
                        mouse.column,
                        mouse.row,
                        git_cards_area(area),
                    ) {
                        state.selected = idx;
                        state.hovered = Some(idx);
                        return GitAction::Run(state.items[idx]);
                    }
                }
                MouseEventKind::Moved
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Down(MouseButton::Left) => {
                    state.hovered = git_card_at(
                        &state.items,
                        mouse.column,
                        mouse.row,
                        git_cards_area(area),
                    );
                }
                _ => {}
            }
            GitAction::Continue
        }
        _ => GitAction::Continue,
    }
}

fn run_git_action(item: GitItem, ctx: &ProjectContext) -> Result<()> {
    match item {
        GitItem::Init => init(ctx),
        GitItem::Gitignore => write_gitignore(ctx),
        GitItem::ConnectRemote => connect_remote(ctx),
        GitItem::Status => show_status(ctx),
        GitItem::Back => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_content_matches_project_type() {
        assert!(gitignore_content(ProjectType::Rust).contains("target/"));
        assert!(gitignore_content(ProjectType::Tauri).contains("src-tauri/target/"));
        assert!(gitignore_content(ProjectType::Flutter).contains(".dart_tool/"));
        assert!(gitignore_content(ProjectType::Kotlin).contains(".gradle/"));
        assert!(gitignore_content(ProjectType::GoWails).contains("frontend/node_modules/"));
        assert!(gitignore_content(ProjectType::Python).contains("__pycache__/"));
        assert!(gitignore_content(ProjectType::Python).contains(".venv/"));
    }

    #[test]
    fn context_gitignore_includes_windows_rules() {
        let windows = gitignore_content_for_platform(ProjectType::Rust);
        assert!(windows.contains("$RECYCLE.BIN/"));
        assert!(windows.contains("*.lnk"));
        assert!(!windows.contains(".Trash-*/"));
    }

    #[test]
    fn context_gitignore_merge_preserves_user_rules_and_is_idempotent() {
        let existing = "# user rules\ncustom-cache/\ntarget/\n";
        let required = gitignore_content_for_platform(ProjectType::Rust);
        let merged = merge_gitignore(existing, &required);
        assert!(merged.starts_with(existing));
        assert!(merged.contains("custom-cache/"));
        assert!(merged.contains("target/"));
        assert!(merged.contains(".directory"));
        assert_eq!(merge_gitignore(&merged, &required), merged);
        assert_eq!(merged.matches("target/").count(), 1);
    }

    /// Ninguna plantilla debe ignorar el código fuente.
    #[test]
    fn gitignore_never_ignores_sources() {
        for pt in [
            ProjectType::Rust,
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::Kotlin,
            ProjectType::Python,
        ] {
            for line in gitignore_content(pt).lines() {
                let l = line.trim();
                assert!(
                    l != "src" && l != "src/" && l != "lib" && l != "lib/",
                    "{pt:?} ignora código fuente: {l}"
                );
            }
        }
    }

    #[test]
    fn looks_like_git_url_accepts_https_and_ssh() {
        assert!(looks_like_git_url("https://github.com/user/repo.git"));
        assert!(looks_like_git_url("git@github.com:user/repo.git"));
        assert!(looks_like_git_url("ssh://git@github.com/user/repo.git"));
        assert!(!looks_like_git_url("github.com/user/repo"));
        assert!(!looks_like_git_url(""));
    }

    #[test]
    fn web_url_normalizes_ssh_and_strips_git_suffix() {
        assert_eq!(
            web_url("git@github.com:user/repo.git").as_deref(),
            Some("https://github.com/user/repo")
        );
        assert_eq!(
            web_url("git@gitlab.com:user/repo.git").as_deref(),
            Some("https://gitlab.com/user/repo")
        );
        assert_eq!(
            web_url("https://github.com/user/repo.git").as_deref(),
            Some("https://github.com/user/repo")
        );
        assert_eq!(web_url("/ruta/local"), None);
    }
}
