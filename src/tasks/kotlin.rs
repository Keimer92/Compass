use std::io::{self, Write};
use std::path::PathBuf;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

use crate::utils::{android as android_utils, cmd, context::{kotlin_android_dir, ProjectContext}, platform};

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

fn read_kotlin_app_meta(ctx: &ProjectContext) -> Option<(String, String)> {
    let android_dir = kotlin_android_dir(ctx);

    // Try to read versionName from build.gradle.kts or build.gradle
    let gradle_files = [
        android_dir.join("build.gradle.kts"),
        android_dir.join("build.gradle"),
    ];
    let mut version = None;
    for gf in &gradle_files {
        if let Ok(content) = std::fs::read_to_string(gf) {
            for line in content.lines() {
                let t = line.trim();
                if t.contains("versionName") {
                    if let Some(v) = t.split("versionName").nth(1) {
                        let v = v.trim().trim_matches('"').trim_matches('\'');
                        // Strip the closing paren if present
                        let v = v.trim_end_matches(')').trim();
                        if !v.is_empty() && v != "null" {
                            version = Some(v.to_string());
                            break;
                        }
                    }
                }
            }
        }
        if version.is_some() { break; }
    }

    // Try to read app name from res/values/strings.xml
    let strings_paths = [
        android_dir.join("src/main/res/values/strings.xml"),
    ];
    let mut name = None;
    for sp in &strings_paths {
        if let Ok(content) = std::fs::read_to_string(sp) {
            for line in content.lines() {
                let t = line.trim();
                if t.contains("app_name") {
                    if let Some(start) = t.find('>') {
                        if let Some(end) = t[start..].find("</") {
                            let val = &t[start+1..start+end].trim();
                            if !val.is_empty() {
                                name = Some(val.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
        if name.is_some() { break; }
    }

    let name = name.unwrap_or_else(|| "App".to_string());
    let version = version.unwrap_or_else(|| "0.0.0".to_string());
    Some((sanitize_app_name(&name), version))
}

fn release_signing_env(ctx: &ProjectContext) -> Option<String> {
    let android_dir = kotlin_android_dir(ctx);
    let props_path = android_dir.join("keystore.properties");
    let content = std::fs::read_to_string(&props_path).ok()?;
    let mut store_file: Option<String> = None;
    let mut store_pass: Option<String> = None;
    let mut key_pass: Option<String> = None;
    let mut key_alias: Option<String> = None;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"');
            match k.trim() {
                "storeFile" => store_file = Some(v.to_string()),
                "storePassword" | "storepass" => store_pass = Some(v.to_string()),
                "keyPassword" | "keypass" => key_pass = Some(v.to_string()),
                "keyAlias" | "alias" => key_alias = Some(v.to_string()),
                _ => {}
            }
        }
    }
    let store_file = store_file?;
    let store_path = if PathBuf::from(&store_file).is_absolute() {
        PathBuf::from(&store_file)
    } else {
        android_dir.join(&store_file)
    };
    if !store_path.exists() {
        return None;
    }
    let store_pass = store_pass.unwrap_or_default();
    let key_pass = key_pass.unwrap_or_else(|| store_pass.clone());
    let key_alias = key_alias?;
    // Gradle lee estas variables del entorno. La sintaxis de assignación
    // difiere: PowerShell usa `$env:X=`, `sh` usa `export X=`.
    if platform::IS_WINDOWS {
        Some(format!(
            "$env:RELEASE_STORE_FILE='{}'; $env:RELEASE_STORE_PASSWORD='{}'; $env:RELEASE_KEY_ALIAS='{}'; $env:RELEASE_KEY_PASSWORD='{}'",
            store_path.display(), store_pass, key_alias, key_pass
        ))
    } else {
        Some(format!(
            "export RELEASE_STORE_FILE='{}'; export RELEASE_STORE_PASSWORD='{}'; export RELEASE_KEY_ALIAS='{}'; export RELEASE_KEY_PASSWORD='{}'",
            store_path.display(), store_pass, key_alias, key_pass
        ))
    }
}

fn gradlew_cmd(args: &str) -> String {
    // En Windows Gradle se invoca a través de `cmd` porque PowerShell no puede
    // ejecutar un `.bat` directamente. En Linux, `./gradlew` (el wrapper POSIX).
    if platform::IS_WINDOWS {
        format!("cmd /c gradlew.bat {}", args)
    } else {
        format!("./gradlew {}", args)
    }
}

/// Detecta si el modulo Gradle de un proyecto Kotlin es `:app` o `:composeApp`
/// (la plantilla por defecto usa `:composeApp`, pero algunos proyectos usan `:app`).
fn gradle_module(ctx: &ProjectContext) -> String {
    if kotlin_android_dir(ctx).file_name().map(|n| n == "app").unwrap_or(false) {
        ":app".to_string()
    } else {
        ":composeApp".to_string()
    }
}

fn ensure_gradle(ctx: &ProjectContext) -> Result<()> {
    // Acepta `gradlew` o `gradlew.bat`: un proyecto clonado entre Windows y
    // Linux conserva sólo uno de los dos wrappers.
    if platform::find_gradlew(&ctx.script_dir).is_none() {
        anyhow::bail!(
            "No se encontró el wrapper de Gradle en la raíz del proyecto.\n\
             Se esperaba `{}`.",
            platform::gradlew_name()
        );
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
        &gradlew_cmd(&format!("{}:run", gradle_module(ctx))),
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

fn extract_main_activity_package(ctx: &ProjectContext) -> Option<String> {
    let kotlin_dir = kotlin_android_dir(ctx).join("src");
    let mut stack = vec![kotlin_dir];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).ok()?;
        for entry in entries {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if entry.file_name() == "MainActivity.kt" {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if let Some(pkg) = trimmed.strip_prefix("package ") {
                            return Some(
                                pkg.trim_end_matches(';').trim().to_string(),
                            );
                        }
                    }
                }
            }
        }
    }
    None
}

/// Construye el comando PowerShell que instala la app de debug y la lanza
/// (compila + instala + abre en el dispositivo).
fn install_and_launch_cmd(ctx: &ProjectContext, serial: &str) -> String {
    let adb_path = ctx
        .adb_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "adb".to_string());

    let namespace = extract_namespace(&ctx.script_dir)
        .unwrap_or_else(|| "com.example.app".to_string());

    let activity = extract_main_activity_package(ctx)
        .map(|p| format!("{}.MainActivity", p))
        .unwrap_or_else(|| format!("{}.MainActivity", namespace));

    if platform::IS_WINDOWS {
        // PowerShell: `$env:X=`, `$LASTEXITCODE`, call-operator `&`.
        format!(
            "$env:ANDROID_SERIAL='{}'; \
             {}; \
             if ($LASTEXITCODE -eq 0) {{ \
                 Write-Host '[OK] Abriendo aplicacion...'; \
                 & '{}' shell am start -n '{}/{}' \
             }}",
            serial,
            gradlew_cmd(&format!("{}:installDebug", gradle_module(ctx))),
            adb_path,
            namespace,
            activity,
        )
    } else {
        // `sh`: `export X=`, `$?`, y sin call-operator.
        format!(
            "export ANDROID_SERIAL='{}'; \
             {}; \
             if [ $? -eq 0 ]; then \
                 echo '[OK] Abriendo aplicacion...'; \
                 '{}' shell am start -n '{}/{}' \
             fi",
            serial,
            gradlew_cmd(&format!("{}:installDebug", gradle_module(ctx))),
            adb_path,
            namespace,
            activity,
        )
    }
}

pub fn run_android(ctx: &ProjectContext) -> Result<()> {
    ensure_gradle(ctx)?;

    let device = android_utils::select_device(ctx)?;
    let serial = &device.serial;

    let cmd = install_and_launch_cmd(ctx, &serial);

    cmd::spawn_new_window(
        "KMP Android (Run)",
        &ctx.script_dir,
        &cmd,
    )?;

    Ok(())
}

pub fn install_debug(ctx: &ProjectContext) -> Result<()> {
    ensure_gradle(ctx)?;

    let device = android_utils::select_device(ctx)?;
    let serial = &device.serial;

    let cmd = install_and_launch_cmd(ctx, &serial);

    cmd::spawn_new_window(
        "KMP Android Install (Debug)",
        &ctx.script_dir,
        &cmd,
    )?;

    Ok(())
}

pub fn build_windows(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Windows (Desktop) con Gradle...\n");
    ensure_gradle(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    let dist_dir = kotlin_android_dir(ctx).join("build").join("compose").join("binaries").join("main");
    let cmd_line = cmd::reveal_on_success(
        &gradlew_cmd(&format!("{}:packageDistributionForCurrentOS", gradle_module(ctx))),
        &dist_dir,
        &["exe"],
        None,
    );

    cmd::spawn_new_window(
        "KMP Desktop Build",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}

pub fn build_apk(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Release)...\n");
    ensure_gradle(ctx)?;

    let android_dir = kotlin_android_dir(ctx);
    let mut gradle = gradlew_cmd(&format!("{}:assembleRelease", gradle_module(ctx)));
    if let Some(env) = release_signing_env(ctx) {
        gradle = format!("{}; {}", env, gradle);
    } else {
        println!("[WARN] No se encontro keystore.properties de release; el build release podria fallar. Usa la tarjeta Keystore para configurar la firma.");
    }
    let rename = read_kotlin_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let apk_dir = android_dir.join("build").join("outputs").join("apk").join("release");
    let cmd_line = cmd::reveal_on_success(
        &gradle,
        &apk_dir,
        &["apk"],
        rename.as_deref(),
    );

    cmd::spawn_new_window(
        "KMP Android APK",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}

pub fn build_aab(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android AAB (Release)...\n");
    ensure_gradle(ctx)?;

    let android_dir = kotlin_android_dir(ctx);
    let mut gradle = gradlew_cmd(&format!("{}:bundleRelease", gradle_module(ctx)));
    if let Some(env) = release_signing_env(ctx) {
        gradle = format!("{}; {}", env, gradle);
    } else {
        println!("[WARN] No se encontro keystore.properties de release; el build release podria fallar. Usa la tarjeta Keystore para configurar la firma.");
    }
    let rename = read_kotlin_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let aab_dir = android_dir.join("build").join("outputs").join("bundle").join("release");
    let cmd_line = cmd::reveal_on_success(
        &gradle,
        &aab_dir,
        &["aab"],
        rename.as_deref(),
    );

    cmd::spawn_new_window(
        "KMP Android AAB",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      LIMPIAR PROYECTO KMP                       ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    let module_dir = kotlin_android_dir(ctx);
    let module_name = module_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "composeApp".to_string());

    println!("Se eliminará:");
    println!("  - build/");
    println!("  - {}/build/", module_name);
    println!("  - .gradle/");
    println!("  - local.properties");
    println!("  - node_modules/");
    println!("  - *.log");
    println!("  - .kotlin/ (caché Kotlin)");
    println!();
    println!("Se conservará:");
    println!("  - {}/src/ (código fuente Kotlin)", module_name);
    println!("  - {}/build.gradle.kts", module_name);
    println!("  - settings.gradle.kts, gradle.properties");
    println!("  - gradle/wrapper/ (wrapper no se borra)");
    println!("  - gradlew + gradlew.bat");
    println!("  - whatsapp_engine/ (motor WhatsApp)");
    println!("  - {}/resources/whatsapp-engine-bundle/ (bundle WhatsApp compilado)", module_name);
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

    // 3. <modulo>/build/
    pb.set_message(format!("Eliminando {}/build/", module_name));
    let module_build = module_dir.join("build");
    if module_build.exists() {
        cmd::remove_dir(&module_build).ok();
        println!("  [OK] {}/build/ eliminado", module_name);
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
