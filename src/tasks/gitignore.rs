use std::io::{self, Write};
use anyhow::Result;
use crate::utils::context::ProjectContext;

const GITIGNORE_CONTENT: &str = r#"# ============================================
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
# Ignorar builds y auto-generados, pero preservar configuración manual
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

# --- Variables de entorno (pueden contener keys) ---
.env
.env.local
.env.*.local

# --- Logs ---
*.log
"#;

pub fn create(ctx: &ProjectContext) -> Result<()> {
    println!();
    println!("==============================================");
    println!("CREAR .gitignore PARA RUST/TAURI");
    println!("==============================================");
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
        pause();
        return Ok(());
    }

    let gitignore_path = ctx.script_dir.join(".gitignore");
    std::fs::write(&gitignore_path, GITIGNORE_CONTENT)?;

    println!();
    println!("[OK] .gitignore creado exitosamente!");
    println!();
    println!("Archivos PRESERVADOS en git (esenciales para compilar):");
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
    println!();
    println!("Para compilar en máquina nueva:");
    println!("  1. git clone <repo>");
    println!("  2. npm install");
    println!("  3. npx tauri android init (solo si compilas para Android)");
    println!("  4. npm run tauri dev (Windows) / tauri android dev (Android)");

    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
