use std::env;
use anyhow::{Context, Result};
use inquire::Select;

use crate::utils::{android as android_utils, cmd, context::ProjectContext, ui};
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

fn dev_host_for_device(device: &android_utils::Device) -> String {
    match &device.kind {
        android_utils::DeviceKind::Emulator { .. } => "10.0.2.2".to_string(),
        android_utils::DeviceKind::Physical => get_pc_ip(),
    }
}

fn create_tauri_config_override(ctx: &ProjectContext, dev_url: &str) -> Result<String> {
    let content = format!(r#"{{ "build": {{ "devUrl": "{}" }} }}"#, dev_url);
    let gen_dir = ctx.script_dir.join("src-tauri\\gen\\android");
    std::fs::create_dir_all(&gen_dir)
        .context("No se pudo crear src-tauri/gen/android")?;
    let config_path = gen_dir.join("tauri.android.dev.json");
    std::fs::write(&config_path, &content)
        .with_context(|| format!("No se pudo escribir {}", config_path.display()))?;
    Ok("src-tauri\\gen\\android\\tauri.android.dev.json".to_string())
}

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    ensure_node_modules(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;

    set_arch_arm64();
    cleanup_build_locks(ctx);
    ensure_ndk(ctx)?;
    ensure_rust_targets()?;

    let selected_device = android_utils::select_device(ctx)?;
    let dev_host = dev_host_for_device(&selected_device);
    let pc_ip = get_pc_ip();

    let target_url = format!("http://{}:1420", dev_host);

    println!();
    println!("==============================================");
    println!("[INFO] Android HMR activado");
    println!("  Dispositivo : {}", selected_device.serial);
    println!("  Dev URL     : {}", target_url);
    println!("==============================================");
    if matches!(selected_device.kind, android_utils::DeviceKind::Physical) {
        println!("1. Asegúrate que el firewall permita el puerto 1420");
        println!("2. La PC y el dispositivo deben estar en la misma red");
        println!("3. IP de tu PC: {}", pc_ip);
        println!("4. Si falla, ejecuta en PowerShell (como Admin):");
        println!("   New-NetFirewallRule -DisplayName \"Vite Dev\" -Direction Inbound -LocalPort 1420 -Protocol TCP -Action Allow");
    }
    println!("==============================================");
    println!();

    println!("[INFO] Creando config override en src-tauri/gen/android/tauri.android.dev.json ...");
    let config_path = create_tauri_config_override(ctx, &target_url)?;

    println!("[INFO] Iniciando Tauri Android Dev con HMR...");
    let cmd = format!(
        "set TAURI_DEV_HOST={} && npx tauri android dev --config {} --no-dev-server-wait",
        dev_host, config_path,
    );

    cmd::spawn_new_window_interactive("Tauri Android HMR", &ctx.script_dir, &cmd)?;

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

    ui::pause();
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

    ui::pause();
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

    let _ = cmd::run("powershell", &[
        "-Command", "Set-Clipboard", "edge://inspect/#devices"
    ]);
    let _ = cmd::run("cmd", &["/c", "start", "msedge", "chrome://inspect/#devices"]);

    if has_device {
        #[cfg(windows)]
        {
            println!("[INFO] Si no ves el dispositivo en Edge, verifica:");
            println!("  1. En edge://inspect, haz clic en 'Discover USB devices'");
            println!("  2. O usa: adb forward tcp:9222 localabstract:chrome_devtools_remote");
            println!("  3. Abre http://localhost:9222/json en Edge");
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
        .context("[ERROR] No se encontró emulator.exe en E:\\SDKs\\Android ni en ANDROID_HOME.")?;

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


