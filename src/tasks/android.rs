use std::env;
use std::io::{self, Write};
use anyhow::{Context, Result};
use inquire::Select;

use crate::utils::{cmd, context::ProjectContext};
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
    cmd::run_in_dir_status("npx", &["tauri", "android", "init"], &ctx.script_dir)?;
    if !ctx.script_dir.join("src-tauri\\gen\\android\\gradlew.bat").exists() {
        anyhow::bail!("[ERROR] No se pudo inicializar Gradle en el proyecto Android.");
    }
    Ok(())
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

    let ndk_version = "27.0.12077973";
    println!("[INFO] Instalando NDK {}...", ndk_version);

    // Accept licenses
    cmd::run(sdk_manager.to_str().unwrap(), &["--licenses"])?;
    cmd::run(sdk_manager.to_str().unwrap(), &["ndk;27.0.12077973"])?;

    let ndk_path = ctx.android_home.as_ref().unwrap().join(format!("ndk\\{}", ndk_version));
    env::set_var("ANDROID_NDK_HOME", &ndk_path);

    println!("[OK] NDK instalado.\n");
    Ok(())
}

fn cleanup_build_locks(ctx: &ProjectContext) {
    let cargo_lock = ctx.script_dir.join("src-tauri\\target\\.cargo-lock");
    if cargo_lock.exists() {
        let _ = std::fs::remove_file(&cargo_lock);
    }
    let gradle_cache = ctx.script_dir.join("src-tauri\\gen\\android\\app\\.gradle");
    if gradle_cache.exists() {
        let _ = cmd::remove_dir(&gradle_cache);
    }
    // Stop java processes
    let _ = cmd::run("taskkill", &["/F", "/IM", "java.exe", "/T"]);
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

fn list_devices(ctx: &ProjectContext) -> Result<String> {
    if let Some(adb) = &ctx.adb_path {
        let output = cmd::run(adb.to_str().unwrap(), &["devices", "-l"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let devices: Vec<String> = stdout.lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty() && !l.contains("offline") && l.contains("device"))
            .map(|l| l.trim().to_string())
            .collect();

        if devices.is_empty() {
            anyhow::bail!(
                "No hay dispositivos Android conectados.\n\
                 Conecta USB con depuración activada."
            );
        }

        println!("\nDispositivos disponibles:");
        for (i, d) in devices.iter().enumerate() {
            println!("  {}. {}", i + 1, d);
        }
        println!();

        let selection = Select::new("Selecciona un dispositivo:", devices.clone())
            .prompt()?;

        Ok(selection)
    } else {
        anyhow::bail!("ADB no encontrado.");
    }
}

fn get_pc_ip() -> String {
    let output = cmd::run("ipconfig", &[]);
    if let Ok(o) = output {
        let stdout = String::from_utf8_lossy(&o.stdout);
        for line in stdout.lines() {
            if line.to_lowercase().contains("ipv4") {
                if let Some(ip) = line.split(':').nth(1) {
                    return ip.trim().to_string();
                }
            }
        }
    }
    "No detectada".to_string()
}

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;

    set_arch_arm64();
    cleanup_build_locks(ctx);
    ensure_ndk(ctx)?;
    ensure_rust_targets()?;

    let selected_device = list_devices(ctx)?;
    let pc_ip = get_pc_ip();

    println!();
    println!("==============================================");
    println!("[INFO] IMPORTANTE PARA DISPOSITIVO FÍSICO:");
    println!("==============================================");
    println!("1. Asegúrate que el firewall permita el puerto 1420");
    println!("2. La PC y el dispositivo deben estar en la misma red");
    println!("3. IP de tu PC: {}", pc_ip);
    println!("4. Si falla, ejecuta en PowerShell (como Admin):");
    println!("   New-NetFirewallRule -DisplayName \"Vite Dev\" -Direction Inbound -LocalPort 1420 -Protocol TCP -Action Allow");
    println!("==============================================");
    println!();
    println!("[OK] Seleccionado: {}", selected_device);
    println!();

    println!("[INFO] Iniciando servidor Vite y Android...");

    cmd::spawn_new_window("Vite Dev Server", &ctx.script_dir, "npm run dev")?;
    std::thread::sleep(std::time::Duration::from_secs(5));
    cmd::spawn_new_window(
        "Tauri Android (Dev)",
        &ctx.script_dir,
        "npx tauri android dev",
    )?;

    Ok(())
}

pub fn build_arm64(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;

    set_arch_arm64();

    println!();
    println!("[INFO] Compilando Android solo arm64 (Release)...");
    println!("[INFO] Usando: npx tauri android build --target aarch64");
    println!();

    cmd::spawn_new_window(
        "Android Build arm64",
        &ctx.script_dir,
        "npx tauri android build --target aarch64",
    )?;

    pause();
    Ok(())
}

pub fn build_all(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;

    clear_arch_env();

    println!();
    println!("[INFO] Compilando Android (Todas las arquitecturas)...");
    println!();

    cmd::spawn_new_window(
        "Android Build (All)",
        &ctx.script_dir,
        "npx tauri android build",
    )?;

    pause();
    Ok(())
}

pub fn debug(_ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║         DEPURADOR DE CHROME (Android)            ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!("[INFO] La URL chrome://inspect/#devices ha sido copiada al portapapeles.");
    println!("[INFO] 1. Abre Chrome (o Edge).");
    println!("[INFO] 2. Pega la URL en la barra de direcciones (Ctrl+V).");
    println!("[INFO] 3. Haz clic en inspect bajo tu dispositivo.");
    println!();

    // Copy to clipboard using PowerShell
    let _ = cmd::run("powershell", &[
        "-Command", "Set-Clipboard", "chrome://inspect/#devices"
    ]);

    // Try to open Chrome
    let _ = cmd::run("cmd", &["/c", "start", "chrome"]);

    pause();
    Ok(())
}

pub fn configure_signing(ctx: &ProjectContext) -> Result<()> {
    signer::create_or_import(ctx)?;
    Ok(())
}

fn pause() {
    use crossterm::event::{read, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{enable_raw_mode, disable_raw_mode};

    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();

    if enable_raw_mode().is_ok() {
        loop {
            if let Ok(Event::Key(key)) = read() {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter {
                    break;
                }
            }
        }
        let _ = disable_raw_mode();
    } else {
        // Fallback: esperar con read_line si raw mode falla
        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
    }
}
