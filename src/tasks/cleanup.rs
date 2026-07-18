use std::io::{self, Write};
use std::path::Path;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::Select;

use crate::utils::{cmd, context::{ProjectContext, ProjectType}};

fn format_size(bytes: u64) -> String {
    if bytes > 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes > 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    }
}

fn dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    fn walk(dir: &Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    total += walk(&path);
                } else if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
        total
    }
    walk(path)
}

fn clean_cache_tauri(ctx: &ProjectContext, aggressive: bool) -> Result<()> {
    if aggressive {
        println!("[INFO] Limpieza agresiva...\n");
        let vite_cache = ctx.script_dir.join("node_modules\\.vite");
        if vite_cache.exists() { std::fs::remove_dir_all(&vite_cache).ok(); }
        let target_dir = ctx.script_dir.join("src-tauri\\target");
        if target_dir.exists() {
            let size = dir_size(&target_dir);
            std::fs::remove_dir_all(&target_dir).ok();
            println!("  [OK] target/ eliminado ({})", format_size(size));
        }
        let android_build = ctx.script_dir.join("src-tauri\\gen\\android\\app\\build");
        if android_build.exists() {
            let size = dir_size(&android_build);
            std::fs::remove_dir_all(&android_build).ok();
            println!("  [OK] Android build eliminado ({})", format_size(size));
        }
        let dist = ctx.script_dir.join("dist");
        if dist.exists() {
            let size = dir_size(&dist);
            std::fs::remove_dir_all(&dist).ok();
            println!("  [OK] dist/ eliminado ({})", format_size(size));
        }
        println!("\n[INFO] Limpiando cache de npm...");
        cmd::run_status("npm", &["cache", "clean", "--force"])?;
        println!("[INFO] Ejecutando cargo clean...");
        cmd::run_in_dir_status("cargo", &["clean"], &ctx.script_dir.join("src-tauri"))?;
    } else {
        println!("\n[INFO] Limpieza rápida (builds)...\n");
        let targets = [
            ("Android build", ctx.script_dir.join("src-tauri\\gen\\android\\app\\build")),
            ("Windows Release", ctx.script_dir.join("src-tauri\\target\\release")),
            ("Windows Debug", ctx.script_dir.join("src-tauri\\target\\debug")),
            ("Dist", ctx.script_dir.join("dist")),
            ("Vite cache", ctx.script_dir.join("node_modules\\.vite")),
        ];
        for (label, path) in &targets {
            if path.exists() {
                let size = dir_size(path);
                if path.is_dir() { std::fs::remove_dir_all(path).ok(); }
                else { std::fs::remove_file(path).ok(); }
                println!("  [OK] {} limpiado ({})", label, format_size(size));
            } else {
                println!("  [--] No hay {}", label.to_lowercase());
            }
        }
    }
    Ok(())
}

fn clean_cache_android_common(ctx: &ProjectContext, aggressive: bool) -> Result<()> {
    if aggressive {
        println!("[INFO] Limpieza agresiva...\n");
        let gradle_cache = ctx.script_dir.join(".gradle");
        if gradle_cache.exists() {
            let size = dir_size(&gradle_cache);
            std::fs::remove_dir_all(&gradle_cache).ok();
            println!("  [OK] .gradle/ eliminado ({})", format_size(size));
        }
        let node_modules = ctx.script_dir.join("node_modules");
        if node_modules.exists() {
            let size = dir_size(&node_modules);
            std::fs::remove_dir_all(&node_modules).ok();
            println!("  [OK] node_modules/ eliminado ({})", format_size(size));
        }
        println!("\n[INFO] Limpiando cache de npm...");
        cmd::run_status("npm", &["cache", "clean", "--force"])?;
    } else {
        println!("\n[INFO] Limpieza rápida (builds)...\n");
        let targets = [
            ("build/ raíz", ctx.script_dir.join("build")),
            ("composeApp/build/", ctx.script_dir.join("composeApp\\build")),
            (".gradle/", ctx.script_dir.join(".gradle")),
            (".kotlin/", ctx.script_dir.join(".kotlin")),
        ];
        for (label, path) in &targets {
            if path.exists() {
                let size = dir_size(path);
                if path.is_dir() { std::fs::remove_dir_all(path).ok(); }
                else { std::fs::remove_file(path).ok(); }
                println!("  [OK] {} limpiado ({})", label, format_size(size));
            } else {
                println!("  [--] No hay {}", label.to_lowercase());
            }
        }
    }
    Ok(())
}

fn clean_cache_flutter(ctx: &ProjectContext, aggressive: bool) -> Result<()> {
    if aggressive {
        println!("[INFO] Limpieza agresiva...\n");
        let build_dir = ctx.script_dir.join("build");
        if build_dir.exists() {
            let size = dir_size(&build_dir);
            std::fs::remove_dir_all(&build_dir).ok();
            println!("  [OK] build/ eliminado ({})", format_size(size));
        }
        let android_build = ctx.script_dir.join("android\\app\\build");
        if android_build.exists() {
            let size = dir_size(&android_build);
            std::fs::remove_dir_all(&android_build).ok();
            println!("  [OK] Android build eliminado ({})", format_size(size));
        }
        let node_modules = ctx.script_dir.join("node_modules");
        if node_modules.exists() {
            let size = dir_size(&node_modules);
            std::fs::remove_dir_all(&node_modules).ok();
            println!("  [OK] node_modules/ eliminado ({})", format_size(size));
        }
        println!("\n[INFO] Limpiando cache de npm...");
        cmd::run_status("npm", &["cache", "clean", "--force"])?;
        println!("[INFO] Ejecutando flutter clean...");
        let _ = cmd::run_in_dir("flutter", &["clean"], &ctx.script_dir);
    } else {
        println!("\n[INFO] Limpieza rápida (builds)...\n");
        let targets = [
            ("build/", ctx.script_dir.join("build")),
            ("Android build", ctx.script_dir.join("android\\app\\build")),
            (".dart_tool/", ctx.script_dir.join(".dart_tool")),
        ];
        for (label, path) in &targets {
            if path.exists() {
                let size = dir_size(path);
                if path.is_dir() { std::fs::remove_dir_all(path).ok(); }
                else { std::fs::remove_file(path).ok(); }
                println!("  [OK] {} limpiado ({})", label, format_size(size));
            } else {
                println!("  [--] No hay {}", label.to_lowercase());
            }
        }
    }
    Ok(())
}

pub fn clean_cache(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      LIMPIEZA DE CACHE Y DEPENDENCIAS            ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let (opt1, opt2) = match ctx.project_type {
        ProjectType::Tauri => ("1. Rápida - Solo limpia builds (Android + Windows)", "2. Agresiva - Limpia TODO (target, node_modules, npm cache, builds)"),
        ProjectType::Flutter => ("1. Rápida - Solo limpia builds (Android + Flutter)", "2. Agresiva - Limpia TODO (build, node_modules, Flutter cache)"),
        ProjectType::Kotlin => ("1. Rápida - Solo limpia builds (Gradle + Kotlin)", "2. Agresiva - Limpia TODO (build, .gradle, node_modules, npm cache)"),
    };

    let options = vec![opt1, opt2];

    let choice = Select::new("Selecciona tipo de limpieza:", options)
        .prompt()?;

    cmd::clear_screen();

    let aggressive = choice.starts_with("2");

    match ctx.project_type {
        ProjectType::Tauri => clean_cache_tauri(ctx, aggressive)?,
        ProjectType::Flutter => clean_cache_flutter(ctx, aggressive)?,
        ProjectType::Kotlin => clean_cache_android_common(ctx, aggressive)?,
    }

    println!("\n[OK] Limpieza completada!");
    pause();
    Ok(())
}

pub fn clean_project(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      LIMPIAR PROYECTO PARA PORTABILIDAD          ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("Se eliminarán TODOS los archivos de compilación y dependencias,\
              dejando solo el código fuente y configuración.");
    println!();
    println!("Se eliminará:");
    println!("  - src-tauri/target/");
    println!("  - src-tauri/gen/android/ (builds, gradle, auto-generados)");
    println!("  - src-tauri/gen/schemas/");
    println!("  - node_modules/");
    println!("  - dist/");
    println!("  - node_modules/.vite/");
    println!("  - *.log, .cargo-lock");
    println!();
    println!("Se conservará:");
    println!("  - src/ (frontend)");
    println!("  - src-tauri/src/ (Rust)");
    println!("  - src-tauri/Cargo.toml + Cargo.lock");
    println!("  - src-tauri/tauri.conf.json, build.rs, icons/, capabilities/");
    println!("  - package.json + package-lock.json");
    println!("  - vite.config.ts, tsconfig*.json, index.html");
    println!("  - public/");
    println!("  - AndroidManifest.xml, *.kt, res/");
    println!("  - Firmas: keystore (*.keystore, keystore.properties)");
    println!("  - Android personalizado: assets/, jniLibs/");
    println!("  - WhatsApp: src-tauri/whatsapp-engine/ + resources/whatsapp-engine-bundle/");
    println!();

    let confirm = inquire::Confirm::new("¿Estás seguro?")
        .with_default(false)
        .prompt()?;

    if !confirm {
        println!("[INFO] Operación cancelada.");
        pause();
        return Ok(());
    }

    let pb = ProgressBar::new(8);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40}] {pos}/{len} {msg}")?
        .progress_chars("=> "));

    let mut total_freed = 0u64;

    // 1. Rust build artifacts
    pb.set_message("Limpiando target/");
    let target_dir = ctx.script_dir.join("src-tauri\\target");
    if target_dir.exists() {
        let size = dir_size(&target_dir);
        std::fs::remove_dir_all(&target_dir).ok();
        total_freed += size;
        println!("  [OK] target/ ({})", format_size(size));
    }
    pb.inc(1);

    // 2. Android auto-generated
    pb.set_message("Limpiando gen/ (preservando config manual)");
    let gen_dir = ctx.script_dir.join("src-tauri\\gen");
    if gen_dir.exists() {
        let size = dir_size(&gen_dir);
        let artifacts = [
            "android\\app\\build",
            "android\\.gradle",
            "android\\build",
            "android\\gradle",
            "android\\buildSrc",
            "android\\app\\proguard-rules.pro",
            "android\\app\\proguard-tauri.pro",
            "android\\app\\tauri.build.gradle.kts",
            "android\\app\\tauri.properties",
            "android\\app\\src\\main\\java\\com\\schooltracker\\proela\\generated",
            "schemas",
        ];
        for rel in &artifacts {
            let full = gen_dir.join(rel);
            if full.exists() {
                if full.is_dir() { std::fs::remove_dir_all(&full).ok(); }
                else { std::fs::remove_file(&full).ok(); }
            }
        }
        println!("  [OK] gen/ ({})", format_size(size));
    }
    pb.inc(1);

    // 3. node_modules
    pb.set_message("Eliminando node_modules/");
    let nm = ctx.script_dir.join("node_modules");
    if nm.exists() {
        let size = dir_size(&nm);
        std::fs::remove_dir_all(&nm).ok();
        total_freed += size;
        println!("  [OK] node_modules/ ({})", format_size(size));
    }
    pb.inc(1);

    // 4. dist
    pb.set_message("Eliminando dist/");
    let dist = ctx.script_dir.join("dist");
    if dist.exists() {
        let size = dir_size(&dist);
        std::fs::remove_dir_all(&dist).ok();
        total_freed += size;
        println!("  [OK] dist/ ({})", format_size(size));
    }
    pb.inc(1);

    // 5. Vite cache
    pb.set_message("Eliminando Vite cache");
    let vite = ctx.script_dir.join("node_modules\\.vite");
    if vite.exists() {
        let size = dir_size(&vite);
        std::fs::remove_dir_all(&vite).ok();
        println!("  [OK] Vite cache ({})", format_size(size));
    }
    pb.inc(1);

    // 6. .cargo-lock
    pb.set_message("Eliminando .cargo-lock");
    let cargo_lock = ctx.script_dir.join("src-tauri\\target\\.cargo-lock");
    if cargo_lock.exists() {
        let size = std::fs::metadata(&cargo_lock).map(|m| m.len()).unwrap_or(0);
        std::fs::remove_file(&cargo_lock).ok();
        total_freed += size;
        println!("  [OK] .cargo-lock ({})", format_size(size));
    }
    pb.inc(1);

    // 7. Build logs
    pb.set_message("Eliminando logs");
    let build_log = ctx.script_dir.join("src-tauri\\build_log.txt");
    if build_log.exists() {
        let size = std::fs::metadata(&build_log).map(|m| m.len()).unwrap_or(0);
        std::fs::remove_file(&build_log).ok();
        total_freed += size;
        println!("  [OK] build_log.txt ({})", format_size(size));
    }
    pb.inc(1);

    // 8. Gradle build (re-check)
    pb.set_message("Verificando Gradle build");
    let gradle_build = ctx.script_dir.join("src-tauri\\gen\\android\\app\\build");
    if gradle_build.exists() {
        let size = dir_size(&gradle_build);
        std::fs::remove_dir_all(&gradle_build).ok();
        total_freed += size;
        println!("  [OK] Gradle build ({})", format_size(size));
    }
    pb.inc(1);

    pb.finish_with_message("Limpieza completada");

    println!();
    println!("[OK] Proyecto limpiado para portabilidad!");
    println!("     Espacio liberado: {}", format_size(total_freed));
    println!();
    println!("Para compilar en una máquina nueva:");
    println!("  1. npm install");
    println!("  2. npx tauri android init  (solo para Android)");
    println!("  3. npm run tauri dev       (Windows)");
    println!("  4. npm run tauri android dev (Android)");

    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
