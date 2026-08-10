use std::io::{self, Write};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

use crate::utils::{android as android_utils, cmd, context::ProjectContext};

fn gradlew_cmd(args: &str) -> String {
    if cfg!(windows) {
        format!("cmd /c gradlew.bat {}", args)
    } else {
        format!("./gradlew {}", args)
    }
}

fn ensure_gradle(ctx: &ProjectContext) -> Result<()> {
    if !ctx.script_dir.join("gradlew.bat").exists() {
        anyhow::bail!("No se encontró gradlew.bat en la raíz del proyecto.");
    }
    Ok(())
}

pub fn run_windows(ctx: &ProjectContext) -> Result<()> {
    ensure_gradle(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║     KMP DESKTOP                                  ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!("[INFO] Iniciando con Hot Reload...");
    println!();
    cmd::spawn_new_window_interactive(
        "KMP Desktop (Run)",
        &ctx.script_dir,
        "gradlew.bat :composeApp:run",
    )?;

    Ok(())
}

fn extract_namespace(project_dir: &std::path::Path) -> Option<String> {
    let paths = [
        project_dir.join("composeApp/build.gradle.kts"),
        project_dir.join("composeApp/build.gradle"),
    ];
    for p in &paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(ns) = trimmed.strip_prefix("namespace = ") {
                    return Some(ns.trim_matches('"').to_string());
                }
            }
        }
    }
    None
}

pub fn run_android(ctx: &ProjectContext) -> Result<()> {
    ensure_gradle(ctx)?;

    let device = android_utils::select_device(ctx)?;
    let serial = &device.serial;

    let adb_path = ctx
        .adb_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "adb".to_string());

    let namespace = extract_namespace(&ctx.script_dir)
        .unwrap_or_else(|| "com.example.app".to_string());

    let cmd = format!(
        "$env:ANDROID_SERIAL='{}'; \
         {}; \
         if ($LASTEXITCODE -eq 0) {{ \
             Write-Host '[OK] Abriendo aplicacion...'; \
             & '{}' shell am start -n '{}/.MainActivity' \
         }}",
        serial,
        gradlew_cmd(":composeApp:installDebug"),
        adb_path,
        namespace,
    );

    cmd::spawn_new_window(
        "KMP Android (Run)",
        &ctx.script_dir,
        &cmd,
    )?;

    Ok(())
}

pub fn build_windows(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Windows (Desktop) con Gradle...\n");
    ensure_gradle(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    let dist_dir = ctx.script_dir.join("composeApp\\build\\compose\\binaries\\main");
    let cmd_line = cmd::reveal_on_success(
        &gradlew_cmd(":composeApp:packageDistributionForCurrentOS"),
        &dist_dir,
        &["exe"],
    );

    cmd::spawn_new_window(
        "KMP Desktop Build",
        &ctx.script_dir,
        &cmd_line,
    )?;

    pause();
    Ok(())
}

pub fn build_apk(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Debug)...\n");
    ensure_gradle(ctx)?;

    let apk_dir = ctx.script_dir.join("composeApp\\build\\outputs\\apk");
    let cmd_line = cmd::reveal_on_success(
        &gradlew_cmd(":composeApp:assembleDebug"),
        &apk_dir,
        &["apk"],
    );

    cmd::spawn_new_window(
        "KMP Android APK",
        &ctx.script_dir,
        &cmd_line,
    )?;

    pause();
    Ok(())
}

pub fn build_aab(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android AAB (Release)...\n");
    ensure_gradle(ctx)?;

    let aab_dir = ctx.script_dir.join("composeApp\\build\\outputs\\bundle\\release");
    let cmd_line = cmd::reveal_on_success(
        &gradlew_cmd(":composeApp:bundleRelease"),
        &aab_dir,
        &["aab"],
    );

    cmd::spawn_new_window(
        "KMP Android AAB",
        &ctx.script_dir,
        &cmd_line,
    )?;

    pause();
    Ok(())
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      LIMPIAR PROYECTO KMP                       ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("Se eliminará:");
    println!("  - build/");
    println!("  - composeApp/build/");
    println!("  - .gradle/");
    println!("  - local.properties");
    println!("  - node_modules/");
    println!("  - *.log");
    println!("  - .kotlin/ (caché Kotlin)");
    println!();
    println!("Se conservará:");
    println!("  - composeApp/src/ (código fuente Kotlin)");
    println!("  - composeApp/build.gradle.kts");
    println!("  - settings.gradle.kts, gradle.properties");
    println!("  - gradle/wrapper/ (wrapper no se borra)");
    println!("  - gradlew + gradlew.bat");
    println!("  - whatsapp_engine/ (motor WhatsApp)");
    println!("  - composeApp/resources/whatsapp-engine-bundle/ (bundle WhatsApp compilado)");
    println!("  - Firmas: keystore.properties, *.keystore, *.jks");
    println!();

    let confirm = inquire::Confirm::new("¿Estás seguro?")
        .with_default(false)
        .prompt()?;

    if !confirm {
        println!("[INFO] Operación cancelada.");
        pause();
        return Ok(());
    }

    let pb = ProgressBar::new(6);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40}] {pos}/{len} {msg}")?
        .progress_chars("=> "));

    // 1. gradle clean
    pb.set_message("Ejecutando gradle clean");
    let _ = if cfg!(windows) {
        cmd::run_in_dir("cmd", &["/c", "gradlew.bat", "clean"], &ctx.script_dir)
    } else {
        cmd::run_in_dir("./gradlew", &["clean"], &ctx.script_dir)
    };
    pb.inc(1);

    // 2. root build/
    pb.set_message("Eliminando build/ raíz");
    let root_build = ctx.script_dir.join("build");
    if root_build.exists() {
        cmd::remove_dir(&root_build).ok();
        println!("  [OK] build/ raíz eliminado");
    }
    pb.inc(1);

    // 3. composeApp/build/
    pb.set_message("Eliminando composeApp/build/");
    let compose_build = ctx.script_dir.join("composeApp\\build");
    if compose_build.exists() {
        cmd::remove_dir(&compose_build).ok();
        println!("  [OK] composeApp/build/ eliminado");
    }
    pb.inc(1);

    // 4. .gradle/
    pb.set_message("Eliminando .gradle/");
    let gradle_cache = ctx.script_dir.join(".gradle");
    if gradle_cache.exists() {
        cmd::remove_dir(&gradle_cache).ok();
        println!("  [OK] .gradle/ eliminado");
    }
    pb.inc(1);

    // 5. node_modules/
    pb.set_message("Eliminando node_modules/");
    let nm = ctx.script_dir.join("node_modules");
    if nm.exists() {
        cmd::remove_dir(&nm).ok();
        println!("  [OK] node_modules/ eliminado");
    }
    pb.inc(1);

    // 6. Kotlin cache + local.properties
    pb.set_message("Eliminando .kotlin/ y local.properties");
    let kotlin_cache = ctx.script_dir.join(".kotlin");
    if kotlin_cache.exists() {
        cmd::remove_dir(&kotlin_cache).ok();
        println!("  [OK] .kotlin/ eliminado");
    }
    let local_props = ctx.script_dir.join("local.properties");
    if local_props.exists() {
        std::fs::remove_file(&local_props).ok();
        println!("  [OK] local.properties eliminado");
    }
    pb.inc(1);

    pb.finish_with_message("Limpieza completada");
    println!("\n[OK] Proyecto KMP limpiado!");
    if cfg!(windows) {
        println!("Para restaurar dependencias: gradlew.bat build");
    } else {
        println!("Para restaurar dependencias: ./gradlew build");
    }

    pause();
    Ok(())
}

fn ensure_whatsapp_npm(ctx: &ProjectContext) -> Result<()> {
    if ctx.has_whatsapp_engine {
        let engine_path = ctx.project_type.whatsapp_path(&ctx.script_dir);
        println!("[INFO] Instalando dependencias del motor WhatsApp...");
        let _ = cmd::run_npm_install(&engine_path);
    }
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
