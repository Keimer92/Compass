use std::io::{self, Write};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

use crate::utils::{android as android_utils, cmd, context::ProjectContext};

fn sanitize_app_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c == ' ' {
                '_'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn read_flutter_app_meta(ctx: &ProjectContext) -> Option<(String, String)> {
    let pubspec_path = ctx.script_dir.join("pubspec.yaml");
    let content = std::fs::read_to_string(&pubspec_path).ok()?;
    let mut name = None;
    let mut version = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name:") && name.is_none() {
            name = trimmed.strip_prefix("name:").map(|v| v.trim().trim_matches('\'').trim_matches('"').to_string());
        }
        if trimmed.starts_with("version:") && version.is_none() {
            if let Some(v) = trimmed.strip_prefix("version:") {
                let v = v.trim().trim_matches('\'').trim_matches('"');
                // Flutter version format: "1.0.0+1" — take only the part before '+'
                version = Some(v.split('+').next().unwrap_or(v).to_string());
            }
        }
        if name.is_some() && version.is_some() {
            break;
        }
    }
    Some((sanitize_app_name(&name?), version?))
}

/// Target de Flutter para desktop en la plataforma actual.
///
/// Antes era `windows` fijo, así que en Linux `flutter run -d windows` fallaba
/// con "no supported devices". Flutter usa `linux` en Linux y `windows` en
/// Windows; `flutter run -d <target>` sin `-d` también valía, pero ser
/// explícito da un error más claro si falta el toolchain del desktop.
fn desktop_target() -> &'static str {
    if crate::utils::platform::IS_WINDOWS { "windows" } else { "linux" }
}

pub fn run_windows(ctx: &ProjectContext) -> Result<()> {
    ensure_pubget(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    cmd::spawn_new_window_interactive(
        "Flutter Desktop (Dev)",
        &ctx.script_dir,
        &format!("flutter run -d {}", desktop_target()),
    )?;

    Ok(())
}

pub fn run_android(ctx: &ProjectContext) -> Result<()> {
    ensure_pubget(ctx)?;

    let device = android_utils::select_device(ctx)?;
    let serial = &device.serial;

    cmd::spawn_new_window_interactive(
        "Flutter Android (Dev)",
        &ctx.script_dir,
        &format!("flutter run -d {}", serial),
    )?;

    Ok(())
}

pub fn profile_windows(ctx: &ProjectContext) -> Result<()> {
    ensure_pubget(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    cmd::spawn_new_window_interactive(
        "Flutter Desktop (Profile)",
        &ctx.script_dir,
        &format!("flutter run --profile -d {}", desktop_target()),
    )?;

    Ok(())
}

pub fn profile_android(ctx: &ProjectContext) -> Result<()> {
    ensure_pubget(ctx)?;

    let device = android_utils::select_device(ctx)?;
    let serial = &device.serial;

    cmd::spawn_new_window_interactive(
        "Flutter Android (Profile)",
        &ctx.script_dir,
        &format!("flutter run --profile -d {}", serial),
    )?;

    Ok(())
}

pub fn build_windows(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando aplicación de escritorio (Release)...\n");
    ensure_pubget(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    let target = desktop_target();
    // Flutter no escribe los artefactos en una ruta fija por plataforma, así que
    // se busca de forma recursiva bajo build/ en vez de hardcodear la de Windows.
    let release_dir = ctx.script_dir.join("build");
    cmd::run_build_and_reveal(
        &format!("flutter build {target} --release"),
        &ctx.script_dir,
        &release_dir,
        crate::utils::platform::desktop_artifact_exts(),
        None,
    )?;

    Ok(())
}

pub fn build_apk(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Release)...\n");
    ensure_pubget(ctx)?;

    let rename = read_flutter_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let apk_dir = ctx.script_dir.join("build/app/outputs/flutter-apk");
    cmd::run_build_and_reveal(
        "flutter build apk --release",
        &ctx.script_dir,
        &apk_dir,
        &["apk"],
        rename.as_deref(),
    )?;

    Ok(())
}

pub fn build_aab(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android AAB (Release)...\n");
    ensure_pubget(ctx)?;

    let rename = read_flutter_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let aab_dir = ctx.script_dir.join("build/app/outputs/bundle/release");
    cmd::run_build_and_reveal(
        "flutter build appbundle --release",
        &ctx.script_dir,
        &aab_dir,
        &["aab"],
        rename.as_deref(),
    )?;

    Ok(())
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      LIMPIAR PROYECTO FLUTTER                    ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("Se eliminará:");
    println!("  - build/");
    println!("  - .dart_tool/");
    println!("  - .packages, .flutter-plugins*");
    println!("  - node_modules/");
    println!("  - dist/");
    println!("  - android/.gradle/");
    println!("  - android/build/");
    println!("  - android/app/build/");
    println!("  - *.log");
    println!();
    println!("Se conservará:");
    println!("  - lib/ (código fuente Dart)");
    println!("  - android/ (configuración, key.properties, assets, jniLibs)");
    println!("  - ios/, web/, test/");
    println!("  - pubspec.yaml + pubspec.lock");
    println!("  - whatsapp_engine/ (motor WhatsApp)");
    println!("  - resources/whatsapp-engine-bundle/ (bundle WhatsApp compilado)");
    println!("  - Firmas: key.properties, *.keystore, *.jks");
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

    // 1. flutter clean
    pb.set_message("Ejecutando flutter clean");
    let _ = cmd::run_in_dir("flutter", &["clean"], &ctx.script_dir);
    pb.inc(1);

    // 2. build/
    pb.set_message("Eliminando build/");
    let build_dir = ctx.script_dir.join("build");
    if build_dir.exists() {
        cmd::remove_dir(&build_dir).ok();
        println!("  [OK] build/ eliminado");
    }
    pb.inc(1);

    // 3. .dart_tool/
    pb.set_message("Eliminando .dart_tool/");
    let dart_tool = ctx.script_dir.join(".dart_tool");
    if dart_tool.exists() {
        cmd::remove_dir(&dart_tool).ok();
        println!("  [OK] .dart_tool/ eliminado");
    }
    pb.inc(1);

    // 4. Android build artifacts
    pb.set_message("Limpiando Android build");
    let android_builds = [
        ctx.script_dir.join("android/.gradle"),
        ctx.script_dir.join("android/build"),
        ctx.script_dir.join("android/app/build"),
    ];
    for p in &android_builds {
        if p.exists() { cmd::remove_dir(p).ok(); }
    }
    println!("  [OK] Android build/ limpiado");
    pb.inc(1);

    // 5. node_modules/
    pb.set_message("Eliminando node_modules/");
    let nm = ctx.script_dir.join("node_modules");
    if nm.exists() {
        cmd::remove_dir(&nm).ok();
        println!("  [OK] node_modules/ eliminado");
    }
    pb.inc(1);

    // 6. Flutter plugin artifacts
    pb.set_message("Eliminando .flutter-plugins");
    for f in &[".flutter-plugins", ".flutter-plugins-dependencies", ".packages"] {
        let fp = ctx.script_dir.join(f);
        if fp.exists() { std::fs::remove_file(fp).ok(); }
    }
    pb.inc(1);

    pb.finish_with_message("Limpieza completada");
    println!("\n[OK] Proyecto Flutter limpiado!");
    println!("Para restaurar dependencias: flutter pub get");

    pause();
    Ok(())
}

fn ensure_pubget(ctx: &ProjectContext) -> Result<()> {
    if !ctx.script_dir.join(".dart_tool").exists() {
        println!("[INFO] Ejecutando flutter pub get...");
        cmd::run_in_dir_status("flutter", &["pub", "get"], &ctx.script_dir)?;
        println!("[OK] flutter pub get completado.\n");
    }
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
