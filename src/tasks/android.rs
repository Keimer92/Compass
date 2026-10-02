use std::env;
use std::io::{self, Write};
use anyhow::{Context, Result};
use inquire::Select;

use crate::utils::{android as android_utils, cmd, context::ProjectContext, platform, ui};
use crate::tasks::signer;

fn ensure_node_modules(ctx: &ProjectContext) -> Result<()> {
    if ctx.has_node_modules {
        return Ok(());
    }
    println!("\n[INFO] node_modules no encontrado. Ejecutando npm install...\n");
    cmd::run_in_dir_status("npm", &["install"], &ctx.script_dir)?;
    println!("[OK] npm install completado.\n");
    Ok(())
}

fn ensure_android_project(ctx: &ProjectContext) -> Result<()> {
    if ctx.has_android_project {
        return Ok(());
    }
    println!("[WARNING] No se encontró el proyecto Android.");
    println!("[INFO] Inicializando proyecto Android con Tauri...");
    run_tauri_android_init(ctx)?;
    if platform::find_gradlew(&ctx.script_dir.join("src-tauri").join("gen").join("android")).is_none() {
        anyhow::bail!("[ERROR] No se pudo inicializar Gradle en el proyecto Android.");
    }
    Ok(())
}

fn run_tauri_android_init(ctx: &ProjectContext) -> Result<()> {
    let status = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/c", "npx", "tauri", "android", "init"])
            .current_dir(&ctx.script_dir)
            .status()
    } else {
        std::process::Command::new("npx")
            .args(["tauri", "android", "init"])
            .current_dir(&ctx.script_dir)
            .status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => anyhow::bail!("tauri android init fallo con codigo: {:?}", s.code()),
        Err(e) => anyhow::bail!("No se pudo ejecutar npx: {}", e),
    }
}

fn ensure_package_consistent(ctx: &ProjectContext) -> Result<()> {
    let conf_path = ctx.script_dir.join("src-tauri/tauri.conf.json");
    let conf_str = std::fs::read_to_string(&conf_path)
        .context("No se pudo leer tauri.conf.json")?;
    let conf: serde_json::Value = serde_json::from_str(&conf_str)
        .context("tauri.conf.json no es JSON valido")?;
    let identifier = conf["identifier"].as_str().unwrap_or("");
    if identifier.is_empty() {
        return Ok(());
    }
    let pkg_path = identifier.replace('.', std::path::MAIN_SEPARATOR_STR);
    // Último segmento del identifier (p.ej. "com.keimer.app" -> "app").
    // Se deriva del identifier y no de `pkg_path` porque éste usa
    // MAIN_SEPARATOR_STR, que en Linux es '/' y no '\\'.
    let pkg_name = identifier.rsplit('.').next().unwrap_or(identifier);
    let expected_dir = ctx
        .script_dir
        .join("src-tauri/gen/android/app/src/main/java")
        .join(&pkg_path);
    if expected_dir.exists() {
        return Ok(());
    }
    println!("\n[WARN] El directorio del paquete Android no coincide con el identifier de tauri.conf.json.");
    println!("       Paquete esperado: {}", identifier);
    println!("       Directorio faltante: {}", expected_dir.display());

    let java_base = ctx.script_dir.join("src-tauri/gen/android/app/src/main/java/com/keimer");
    let old_dirs: Vec<_> = if java_base.exists() {
        std::fs::read_dir(&java_base)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name != "schooltracker" && !name.starts_with('.')
            })
            .collect()
    } else {
        vec![]
    };

    if let Some(old_dir) = old_dirs.first() {
        let old_name = old_dir.file_name().to_string_lossy().to_string();
        println!("\nSe detecto directorio antiguo: com.keimer.{}", old_name);
        println!("Opciones:");
        println!("  1) Mover archivos de '{}' a '{}' (preserva todo)", old_name, pkg_name);
        println!("  2) Ejecutar 'tauri android init' (sobreescribe archivos default)");
        println!("  3) Saltar (el build puede fallar)");
        print!("Elige [1/2/3]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        match input {
            "1" => {
                let new_dir = ctx.script_dir
                    .join("src-tauri/gen/android/app/src/main/java")
                    .join(&pkg_path);
                std::fs::create_dir_all(&new_dir)
                    .context("No se pudo crear el directorio del paquete")?;
                let old_java = ctx.script_dir
                    .join("src-tauri/gen/android/app/src/main/java/com/keimer")
                    .join(&old_name);
                if let Ok(entries) = std::fs::read_dir(&old_java) {
                    for entry in entries.flatten() {
                        let file_name = entry.file_name();
                        let dest = new_dir.join(&file_name);
                        if file_name.to_string_lossy().ends_with(".kt") {
                            let content = std::fs::read_to_string(entry.path())
                                .unwrap_or_default();
                            // `identifier` ya viene en notación de paquete
                            // (`com.keimer.app`), que es lo que debe aparecer
                            // en las declaraciones `package` de Kotlin.
                            let new_content = content
                                .replace(
                                    &format!("package com.keimer.{}", old_name),
                                    &format!("package {}", identifier),
                                )
                                .replace(
                                    &format!("com.keimer.{}", old_name),
                                    &format!("com.{}", identifier),
                                );
                            std::fs::write(&dest, new_content)?;
                        } else {
                            std::fs::copy(entry.path(), &dest)?;
                        }
                    }
                }
                println!("[OK] Archivos movidos de 'com.keimer.{}' a '{}'", old_name, identifier);
                println!("[INFO] Puedes eliminar el directorio viejo manualmente si lo deseas.");
            }
            "2" => {
                println!("[INFO] Ejecutando tauri android init...");
                if let Err(e) = run_tauri_android_init(ctx) {
                    println!("[ERROR] {}", e);
                } else {
                    println!("[OK] Proyecto Android regenerado.");
                }
            }
            _ => {
                println!("[INFO] Continuando sin regenerar; el build puede fallar.");
            }
        }
    } else {
        println!("\nOpciones:");
        println!("  1) Ejecutar 'tauri android init' (genera proyecto default)");
        println!("  2) Saltar (el build puede fallar)");
        print!("Elige [1/2]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        match input {
            "1" => {
                println!("[INFO] Ejecutando tauri android init...");
                if let Err(e) = run_tauri_android_init(ctx) {
                    println!("[ERROR] {}", e);
                } else {
                    println!("[OK] Proyecto Android regenerado.");
                }
            }
            _ => {
                println!("[INFO] Continuando sin regenerar; el build puede fallar.");
            }
        }
    }
    ui::pause();
    Ok(())
}

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

fn read_tauri_app_meta(ctx: &ProjectContext) -> Option<(String, String)> {
    let conf_path = ctx.script_dir.join("src-tauri/tauri.conf.json");
    let conf_str = std::fs::read_to_string(&conf_path).ok()?;
    let conf: serde_json::Value = serde_json::from_str(&conf_str).ok()?;
    let name = conf["productName"].as_str()?;
    let version = conf["version"].as_str()?;
    Some((sanitize_app_name(name), version.to_string()))
}

fn ensure_rust_targets() -> Result<()> {
    let output = cmd::run("rustup", &["target", "list", "--installed"])?;
    let installed = String::from_utf8_lossy(&output.stdout);
    let required = [
        "aarch64-linux-android",
        "armv7-linux-androideabi",
        "i686-linux-android",
        "x86_64-linux-android",
    ];

    let missing: Vec<&str> = required.iter().filter(|t| !installed.contains(*t)).copied().collect();
    if missing.is_empty() {
        return Ok(());
    }

    println!("[INFO] Instalando targets de Rust para Android: {}...", missing.join(", "));
    let mut args = vec!["target", "add"];
    args.extend(missing);
    cmd::run_status("rustup", &args)?;
    println!("[OK] Targets de Rust instalados.\n");
    Ok(())
}

fn ensure_ndk(ctx: &ProjectContext) -> Result<()> {
    if let Some(ndk) = &ctx.ndk_home {
        if ndk.join("source.properties").exists() {
            env::set_var("ANDROID_NDK_HOME", ndk);
            return Ok(());
        }
    }

    let sdk_manager = ctx.sdk_manager.as_ref()
        .context("[ERROR] No se encontró sdkmanager. Verifica ANDROID_HOME.")?;

    let ndk_version = platform::NDK_VERSION;
    println!("[INFO] Instalando NDK {}...", ndk_version);

    // Accept licenses
    cmd::run(sdk_manager.to_str().unwrap(), &["--licenses"])?;
    cmd::run(sdk_manager.to_str().unwrap(), &[&format!("ndk;{}", ndk_version)])?;

    let ndk_path = ctx.android_home.as_ref().unwrap().join("ndk").join(ndk_version);
    env::set_var("ANDROID_NDK_HOME", &ndk_path);

    println!("[OK] NDK instalado.\n");
    Ok(())
}

fn cleanup_build_locks(ctx: &ProjectContext) {
    let cargo_lock = ctx.script_dir.join("src-tauri").join("target").join(".cargo-lock");
    if cargo_lock.exists() {
        let _ = std::fs::remove_file(&cargo_lock);
    }
    let gradle_cache = ctx
        .script_dir
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join(".gradle");
    if gradle_cache.exists() {
        let _ = cmd::remove_dir(&gradle_cache);
    }
    // Stop java processes
    if platform::IS_WINDOWS {
        let _ = cmd::run("taskkill", &["/F", "/IM", "java.exe", "/T"]);
    } else {
        let _ = cmd::run("pkill", &["-f", "java"]);
    }
}

fn set_arch_arm64() {
    env::set_var("ORG_GRADLE_PROJECT_abiList", "arm64-v8a");
    env::set_var("ORG_GRADLE_PROJECT_archList", "arm64");
    env::set_var("ORG_GRADLE_PROJECT_targetList", "aarch64");
}

fn clear_arch_env() {
    env::remove_var("ORG_GRADLE_PROJECT_abiList");
    env::remove_var("ORG_GRADLE_PROJECT_archList");
    env::remove_var("ORG_GRADLE_PROJECT_targetList");
}

/// Detecta la IP LAN real para `TAURI_DEV_HOST`.
///
/// Se excluyen loopback (`127.*`) y APIPA (`169.254.*`), que es lo que hacía
/// que Tauri seleccionara una IP incorrecta. En Windows usa `Get-NetIPAddress`
/// (solo Windows); en Linux, `hostname -I`.
fn detect_lan_ip() -> Option<String> {
    let raw = if platform::IS_WINDOWS {
        let output = cmd::run("powershell", &[
            "-NoProfile", "-Command",
            "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' -and $_.PrefixOrigin -ne 'WellKnown' } | Select-Object -First 1).IPAddress"
        ]).ok()?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        // `hostname -I` lista todas las IPv4 asignadas; el filtro es el mismo.
        let output = cmd::run("sh", &["-c", "hostname -I 2>/dev/null"]).ok()?;
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .find(|ip| !ip.starts_with("127.") && !ip.starts_with("169.254."))
            .unwrap_or_default()
            .to_string()
    };

    let ip = raw.trim().to_string();
    if ip.is_empty() || ip.contains("Error") { None } else { Some(ip) }
}

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;
    ensure_package_consistent(ctx)?;

    set_arch_arm64();
    cleanup_build_locks(ctx);
    ensure_ndk(ctx)?;
    ensure_rust_targets()?;

    // Select device (launches emulator if needed)
    let _device = android_utils::select_device(ctx)?;

    ui::info_banner("Iniciando desarrollo Android");
    println!("  Tauri se conectará automáticamente al dispositivo disponible.");
    println!();

    let dev_cmd = match detect_lan_ip() {
        Some(ip) => {
            println!("[INFO] IP LAN detectada: {}", ip);
            env::set_var("TAURI_DEV_HOST", &ip);
            // `spawn_new_window_interactive` ejecuta este fragmento en `cmd` en
            // Windows y en `sh -lc` en Linux, así que la sintaxis de assignación
            // del environment es distinta en cada shell.
            if platform::IS_WINDOWS {
                format!("set TAURI_DEV_HOST={} & npx tauri android dev", ip)
            } else {
                format!("TAURI_DEV_HOST={} npx tauri android dev", ip)
            }
        }
        None => {
            println!("[WARN] No se detectó IP LAN. Tauri pedirá seleccionar manualmente.");
            "npx tauri android dev".to_string()
        }
    };

    cmd::spawn_new_window_interactive(
        "Tauri Android Dev",
        &ctx.script_dir,
        &dev_cmd,
    )?;

    Ok(())
}

pub fn build_arm64(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;
    ensure_package_consistent(ctx)?;

    set_arch_arm64();

    println!();
    println!("[INFO] Compilando Android solo arm64 (Release)...");
    println!("[INFO] Usando: npx tauri android build --target aarch64");
    println!();

    let rename = read_tauri_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let apk_dir = ctx.script_dir.join("src-tauri/gen/android/app/build/outputs/apk");
    let cmd_line = cmd::reveal_on_success("npx tauri android build --target aarch64", &apk_dir, &["apk"], rename.as_deref());

    cmd::spawn_new_window(
        "Android Build arm64",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}

fn set_arch_armv7() {
    env::set_var("ORG_GRADLE_PROJECT_abiList", "armeabi-v7a");
    env::set_var("ORG_GRADLE_PROJECT_archList", "arm");
    env::set_var("ORG_GRADLE_PROJECT_targetList", "armv7");
}

pub fn build_armv7(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;

    set_arch_armv7();

    println!();
    println!("[INFO] Compilando Android solo armv7 (Release)...");
    println!("[INFO] Usando: npx tauri android build --target armv7");
    println!();

    let apk_dir = ctx.script_dir.join("src-tauri/gen/android/app/build/outputs/apk");
    let cmd_line = cmd::reveal_on_success("npx tauri android build --target armv7", &apk_dir, &["apk"], None);

    cmd::spawn_new_window(
        "Android Build armv7",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}

pub fn build_all(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;
    ensure_package_consistent(ctx)?;

    clear_arch_env();

    println!();
    println!("[INFO] Compilando Android (Todas las arquitecturas)...");
    println!();

    let rename = read_tauri_app_meta(ctx).map(|(name, ver)| format!("{}-v{}", name, ver));
    let apk_dir = ctx.script_dir.join("src-tauri/gen/android/app/build/outputs/apk");
    let cmd_line = cmd::reveal_on_success("npx tauri android build", &apk_dir, &["apk"], rename.as_deref());

    cmd::spawn_new_window(
        "Android Build (All)",
        &ctx.script_dir,
        &cmd_line,
    )?;

    Ok(())
}

pub fn debug(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::info_banner("Depurador Chrome (Android)");

    let adb = ctx.adb_path.as_ref()
        .context("[ERROR] ADB no encontrado. Verifica ANDROID_HOME.")?;
    let adb_str = adb.to_str().unwrap();

    println!("[1/4] Reiniciando servidor ADB...");
    cmd::run(adb_str, &["kill-server"]).ok();
    std::thread::sleep(std::time::Duration::from_secs(1));
    cmd::run(adb_str, &["start-server"])?;
    std::thread::sleep(std::time::Duration::from_secs(2));

    println!("[2/4] Buscando dispositivos...");
    let output = cmd::run(adb_str, &["devices", "-l"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{}", stdout);

    let has_device = stdout.lines().skip(1).any(|l| {
        let t = l.trim();
        !t.is_empty() && t.contains("device") && !t.contains("offline")
    });

    if !has_device {
        println!("\n[WARNING] Ningún dispositivo Android detectado.");
        println!("  Asegúrate que el emulador esté encendido o el dispositivo físico conectado.");
        println!("  Luego abre chrome://inspect/#devices manualmente.\n");
    } else {
        println!("[3/4] Abriendo puerto de depuración...");
        for line in stdout.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.contains("offline") || !trimmed.contains("device") {
                continue;
            }
            let serial = trimmed.split_whitespace().next().unwrap_or("");
            if !serial.is_empty() {
                let _ = cmd::run(adb_str, &["-s", serial, "forward", "tcp:9222", "localabstract:chrome_devtools_remote"]);
                let _ = cmd::run(adb_str, &["-s", serial, "forward", "tcp:9229", "localabstract:firefox_devtools_remote"]);
            }
        }
        println!("[4/4] Abriendo Chrome DevTools...");
    }

    println!();
    println!("[INFO] Abriendo edge://inspect/#devices ...");

    if platform::IS_WINDOWS {
        let _ = cmd::run("powershell", &[
            "-Command", "Set-Clipboard", "edge://inspect/#devices"
        ]);
        let _ = cmd::run("cmd", &["/c", "start", "msedge", "chrome://inspect/#devices"]);
    } else {
        // Linux: copiar al portapapeles vía wl-copy/xclip/xsel y abrir con xdg-open.
        let url = "chrome://inspect/#devices";
        let copiers: &[&[&str]] = &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "--clipboard", "--input"],
        ];
        for args in copiers {
            let mut argv: Vec<&str> = args.to_vec();
            argv.push(url);
            if cmd::run(args[0], &argv).is_ok() {
                break;
            }
        }
        let _ = cmd::run("xdg-open", &[url]);
    }

    if has_device {
        #[cfg(windows)]
        {
            println!("[INFO] Si no ves el dispositivo en Edge, verifica:");
            println!("  1. En edge://inspect, haz clic en 'Discover USB devices'");
            println!("  2. O usa: adb forward tcp:9222 localabstract:chrome_devtools_remote");
            println!("  3. Abre http://localhost:9222/json en Edge");
        }
        #[cfg(not(windows))]
        {
            println!("[INFO] Si no ves el dispositivo en Chrome, verifica:");
            println!("  1. En chrome://inspect, activa 'Discover USB devices'");
            println!("  2. O usa: adb forward tcp:9222 localabstract:chrome_devtools_remote");
            println!("  3. Abre http://localhost:9222/json en Chrome");
        }
    }

    ui::pause();
    Ok(())
}

pub fn configure_signing(ctx: &ProjectContext) -> Result<()> {
    signer::create_or_import(ctx)?;
    Ok(())
}

pub fn launch(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::info_banner("Lanzar Emulador Android");

    let emu_path = ctx.emulator_path.as_ref()
        .context("[ERROR] No se encontró el emulador de Android. Verifica ANDROID_HOME o instálalo con sdkmanager.")?;

    let avds = android_utils::list_avds(emu_path)?;

    if avds.is_empty() {
        anyhow::bail!("No hay AVDs creados. Créalos con Android Studio: Tools > AVD Manager.");
    }

    let selected_avd = if avds.len() == 1 {
        let name = &avds[0];
        println!("[INFO] Único AVD detectado: {}", name);
        name.clone()
    } else {
        let prompt = format!("Selecciona un AVD ({} disponibles):", avds.len());
        Select::new(&prompt, avds).prompt()?
    };

    println!("[OK] AVD seleccionado: {}", selected_avd);
    println!();

    android_utils::launch_emulator(emu_path, &selected_avd)?;

    if let Some(adb) = &ctx.adb_path {
        let serial = android_utils::wait_for_emulator_serial(adb, 120)?;
        android_utils::wait_for_emulator_boot(adb, &serial, 120)?;
    }

    ui::success_banner("Emulador listo para pruebas");
    ui::pause();
    Ok(())
}


