use std::io::{self, Write};
use anyhow::Result;
use crate::utils::{cmd, context::{ProjectContext, ProjectType}};

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
    }
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
    }
}

pub fn create(ctx: &ProjectContext) -> Result<()> {
    println!();
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    match ctx.project_type {
        ProjectType::Rust => println!("║        CREAR .gitignore PARA RUST                 ║"),
        ProjectType::Tauri => println!("║        CREAR .gitignore PARA RUST/TAURI          ║"),
        ProjectType::Flutter => println!("║        CREAR .gitignore PARA FLUTTER/DART         ║"),
        ProjectType::Kotlin => println!("║   CREAR .gitignore PARA KOTLIN MULTIPLATFORM       ║"),
    }
    println!("╚══════════════════════════════════════════════════╝");
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
    std::fs::write(&gitignore_path, gitignore_content(ctx.project_type))?;

    println!();
    println!("[OK] .gitignore creado exitosamente!");
    println!();
    println!("Archivos PRESERVADOS en git (esenciales para compilar):");
    print_preserved(ctx.project_type);
    println!();
    println!("Para compilar en máquina nueva:");
    print_instructions(ctx.project_type);

    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
