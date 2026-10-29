use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};

use crate::utils::{
    android as android_utils, cmd,
    context::{kotlin_android_dir, ProjectContext},
    platform, ui,
};

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
        if version.is_some() {
            break;
        }
    }

    // Try to read app name from res/values/strings.xml
    let strings_paths = [android_dir.join("src/main/res/values/strings.xml")];
    let mut name = None;
    for sp in &strings_paths {
        if let Ok(content) = std::fs::read_to_string(sp) {
            for line in content.lines() {
                let t = line.trim();
                if t.contains("app_name") {
                    if let Some(start) = t.find('>') {
                        if let Some(end) = t[start..].find("</") {
                            let val = &t[start + 1..start + end].trim();
                            if !val.is_empty() {
                                name = Some(val.to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
        if name.is_some() {
            break;
        }
    }

    let name = name.unwrap_or_else(|| "App".to_string());
    let version = version.unwrap_or_else(|| "0.0.0".to_string());
    Some((sanitize_app_name(&name), version))
}

fn release_signing_env(ctx: &ProjectContext) -> Option<Vec<(String, String)>> {
    let android_dir = kotlin_android_dir(ctx);
    let props_path = android_dir.join("keystore.properties");
    // Un único parser de properties para todo el crate (`signer::Props`), en
    // vez de la copia que vivía aquí.
    let props = crate::tasks::signer::Props::read(&props_path)?;
    let store_file = props.store_file?;
    let store_path = crate::tasks::signer::resolve_keystore(&android_dir, &store_file);
    if !store_path.exists() {
        return None;
    }
    let store_pass = props.store_password.unwrap_or_default();
    let key_pass = props.key_password.unwrap_or_else(|| store_pass.clone());
    let key_alias = props.key_alias?;
    // Gradle lee estas variables del entorno. Se inyectan con `Command::env`
    // (ver `cmd::run_shell_status`), no prefijando el comando con `$env:…;`
    // (PowerShell) o `export …;` (sh), porque el build corre en la ventana actual.
    Some(vec![
        (
            "RELEASE_STORE_FILE".to_string(),
            store_path.display().to_string(),
        ),
        ("RELEASE_STORE_PASSWORD".to_string(), store_pass),
        ("RELEASE_KEY_ALIAS".to_string(), key_alias),
        ("RELEASE_KEY_PASSWORD".to_string(), key_pass),
    ])
}

/// Comando para invocar el wrapper de Gradle en la raíz del proyecto.
///
/// Ejecuta Gradle mediante `cmd`, que es necesario para invocar `.bat` en Windows.
fn gradlew_cmd(_ctx: &ProjectContext, args: &str) -> String {
    format!("cmd /c gradlew.bat {}", args)
}

/// Detecta si el modulo Gradle de un proyecto Kotlin es `:app` o `:composeApp`
/// (la plantilla por defecto usa `:composeApp`, pero algunos proyectos usan `:app`).
fn gradle_module(ctx: &ProjectContext) -> String {
    if kotlin_android_dir(ctx)
        .file_name()
        .map(|n| n == "app")
        .unwrap_or(false)
    {
        ":app".to_string()
    } else {
        ":composeApp".to_string()
    }
}

fn ensure_gradle(ctx: &ProjectContext) -> Result<()> {
    // Los proyectos Kotlin requieren el wrapper de Windows.
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
        &gradlew_cmd(ctx, &format!("{}:run", gradle_module(ctx))),
    )?;

    Ok(())
}

/// Extrae `namespace = "..."` del módulo Gradle Android.
///
/// Se resuelven los dos nombres de módulo (`app` y `composeApp`) porque
/// `kotlin_android_dir` ya los distingue: hardcodear `composeApp/` hacía que los
/// proyectos con módulo `:app` cayesen al namespace de ejemplo y el
/// `am start` posterior apuntara a una actividad inexistente.
fn extract_namespace(ctx: &ProjectContext) -> Option<String> {
    let module_dir = kotlin_android_dir(ctx);
    let paths = [
        module_dir.join("build.gradle.kts"),
        module_dir.join("build.gradle"),
    ];
    for p in &paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(ns) = trimmed.strip_prefix("namespace = ") {
                    let ns = ns
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_end_matches(';');
                    if !ns.is_empty() {
                        return Some(ns.to_string());
                    }
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
                            return Some(pkg.trim_end_matches(';').trim().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Instala la app de debug en el dispositivo y la lanza (compila + instala + abre).
///
/// Corre en la ventana actual en vez de abrir otra con PowerShell: el `am start`
/// solo se ejecuta si la instalación tuvo éxito.
fn install_and_launch(ctx: &ProjectContext, serial: &str) -> Result<()> {
    let adb_path = ctx
        .adb_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "adb".to_string());

    let namespace = extract_namespace(ctx).unwrap_or_else(|| "com.example.app".to_string());

    let activity = extract_main_activity_package(ctx)
        .map(|p| format!("{}.MainActivity", p))
        .unwrap_or_else(|| format!("{}.MainActivity", namespace));

    let gradle = gradlew_cmd(ctx, &format!("{}:installDebug", gradle_module(ctx)));
    let envs = vec![("ANDROID_SERIAL".to_string(), serial.to_string())];

    println!();
    println!("[INFO] Ejecutando: {gradle}");
    println!();
    if cmd::run_shell_status(&gradle, &ctx.script_dir, &envs)? {
        println!();
        println!("[OK] Abriendo aplicacion...");
        let _ = cmd::run_shell_status(
            &format!(
                "\"{}\" shell am start -n {}/{}",
                adb_path, namespace, activity
            ),
            &ctx.script_dir,
            &[],
        )?;
    }

    println!();
    ui::pause();
    Ok(())
}

pub fn run_android(ctx: &ProjectContext) -> Result<()> {
    ensure_gradle(ctx)?;

    let device = android_utils::select_device(ctx)?;
    install_and_launch(ctx, &device.serial)?;

    Ok(())
}

pub fn install_debug(ctx: &ProjectContext) -> Result<()> {
    ensure_gradle(ctx)?;

    let device = android_utils::select_device(ctx)?;
    install_and_launch(ctx, &device.serial)?;

    Ok(())
}

/// Instala un APK ya compilado (release) en el teléfono y abre la app.
///
/// Es el equivalente del `:installDebug` de Gradle, pero para un artefacto
/// concreto: Gradle no tiene tarea para "instalar este APK release", así que
/// se usa `adb install -r` directamente (ver `android_utils::install_apk`,
/// que además resuelve el choque de firmas debug ↔ release).
fn install_release_apk(ctx: &ProjectContext, apk: &std::path::Path) -> Result<()> {
    let namespace = extract_namespace(ctx).unwrap_or_default();
    let activity = extract_main_activity_package(ctx)
        .map(|p| format!("{}.MainActivity", p))
        .unwrap_or_else(|| format!("{}.MainActivity", namespace));

    // Se instala **solo si hay un dispositivo Android conectado**. Si no lo
    // hay, el paso se omite sin preguntar: la APK ya está compilada.
    android_utils::install_apk_if_device(ctx, apk, &namespace, &activity)?;
    Ok(())
}

pub fn build_windows(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando aplicación de escritorio con Gradle...\n");
    ensure_gradle(ctx)?;
    ensure_whatsapp_npm(ctx)?;

    let dist_dir = kotlin_android_dir(ctx)
        .join("build")
        .join("compose")
        .join("binaries")
        .join("main");
    cmd::run_build_and_reveal(
        &gradlew_cmd(
            ctx,
            &format!("{}:packageDistributionForCurrentOS", gradle_module(ctx)),
        ),
        &ctx.script_dir,
        &dist_dir,
        platform::desktop_artifact_exts(),
        read_kotlin_app_meta(ctx)
            .map(|(name, ver)| format!("{} v{}", name, ver))
            .as_deref(),
    )?;

    Ok(())
}

pub fn build_apk(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Release)...\n");
    ensure_gradle(ctx)?;

    let android_dir = kotlin_android_dir(ctx);
    let gradle = gradlew_cmd(ctx, &format!("{}:assembleRelease", gradle_module(ctx)));
    let envs = match release_signing_env(ctx) {
        Some(envs) => envs,
        None => {
            println!("[WARN] No se encontro keystore.properties de release; el build release podria fallar. Usa la tarjeta Keystore para configurar la firma.");
            Vec::new()
        }
    };
    let rename = read_kotlin_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let apk_dir = android_dir
        .join("build")
        .join("outputs")
        .join("apk")
        .join("release");
    let outcome = cmd::run_build_and_reveal_with_env_outcome(
        &gradle,
        &ctx.script_dir,
        &apk_dir,
        &["apk"],
        rename.as_deref(),
        &envs,
    )?;

    // El APK release se instala en el telefono igual que el de debug: **si
    // hay un dispositivo Android conectado**. Si no lo hay, el paso se omite
    // (la APK ya está compilada) y no se pregunta nada.
    if outcome.success {
        if let Some(apk) = outcome.artifact {
            install_release_apk(ctx, &apk)?;
        } else {
            println!("[WARN] No se encontro el APK para instalar.");
        }
    }

    // Pausa final: la ventana del menú se cierra al terminar el proceso, y
    // sin esto el resultado de la instalación se pierde de vista.
    ui::pause();
    Ok(())
}

pub fn build_aab(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android AAB (Release)...\n");
    ensure_gradle(ctx)?;

    let android_dir = kotlin_android_dir(ctx);
    let gradle = gradlew_cmd(ctx, &format!("{}:bundleRelease", gradle_module(ctx)));
    let envs = match release_signing_env(ctx) {
        Some(envs) => envs,
        None => {
            println!("[WARN] No se encontro keystore.properties de release; el build release podria fallar. Usa la tarjeta Keystore para configurar la firma.");
            Vec::new()
        }
    };
    let rename = read_kotlin_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let aab_dir = android_dir
        .join("build")
        .join("outputs")
        .join("bundle")
        .join("release");
    let outcome = cmd::run_build_and_reveal_with_env_outcome(
        &gradle,
        &ctx.script_dir,
        &aab_dir,
        &["aab"],
        rename.as_deref(),
        &envs,
    )?;

    if outcome.success {
        // Un AAB no se puede instalar con `adb install`: es un bundle de
        // Play Store. Para instalar en el telefono hay que usar el APK
        // (tarjeta de APK release) o `bundletool`.
        println!("[INFO] Un AAB no se instala directamente en el telefono.");
        println!(
            "       Usa la tarjeta de APK release para instalarlo, o sube el bundle a Play Store."
        );
    }

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
    println!("  - gradlew.bat (wrapper de Windows)");
    println!("  - whatsapp_engine/ (motor WhatsApp)");
    println!(
        "  - {}/resources/whatsapp-engine-bundle/ (bundle WhatsApp compilado)",
        module_name
    );
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
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40}] {pos}/{len} {msg}")?
            .progress_chars("=> "),
    );

    // 1. gradle clean
    pb.set_message("Ejecutando gradle clean");
    let _ = cmd::run_in_dir("cmd", &["/c", "gradlew.bat", "clean"], &ctx.script_dir);
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
    println!("Para restaurar dependencias: gradlew.bat build");

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
