use std::env;
use std::io::{self, Write};
use anyhow::{Context, Result};

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

/// Verifica que el directorio del paquete Java generado por Tauri coincida con
/// el `identifier` de `tauri.conf.json`, y ofrece repararlo.
///
/// El error típico que evita es "Project directory …/src/main/java/<pkg> does
/// not exist", que aparece cuando se cambia el `identifier` sin regenerar el
/// proyecto Android con `tauri android init`.
///
/// La búsqueda del directorio antiguo es genérica: recorre el árbol `java/`
/// buscando el directorio hoja cuyo nombre sea el último segmento del
/// `identifier` *anterior*. Antes estaba fija a `com/keimer`, así que en
/// cualquier otro proyecto la rama de "mover archivos" era inalcanzable, y el
/// reemplazo de texto generaba `com.com.keimer.app` (doble `com.`).
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
    // `java/` siempre usa `/` como separador, también en Windows, porque es el
    // layout que Gradle espera; MAIN_SEPARATOR sería `\` y rompería la ruta.
    let pkg_path = identifier.replace('.', "/");
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

    let java_root = ctx.script_dir.join("src-tauri/gen/android/app/src/main/java");

    // Busca cualquier directorio hoja bajo `java/` que contenga archivos .kt.
    // Ése es el paquete generado que quedó con el identifier anterior.
    if let Some(old_leaf) = find_kotlin_package(&java_root) {
        // El paquete antiguo se reconstruye desde el propio directorio, no desde
        // un prefijo supuesto.
        let old_package = package_of_dir(&java_root, &old_leaf)
            .map(|p| p.replace('/', "."))
            .unwrap_or_else(|| old_leaf.file_name().unwrap_or_default().to_string_lossy().into_owned());

        println!("\nSe detecto el paquete anterior: {}", old_package);
        println!("Opciones:");
        println!("  1) Mover '{}' a '{}' y reescribir los 'package'", old_package, identifier);
        println!("  2) Ejecutar 'tauri android init' (sobreescribe archivos default)");
        println!("  3) Saltar (el build puede fallar)");
        print!("Elige [1/2/3]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        match input.trim() {
            "1" => {
                move_package(&old_leaf, &expected_dir, &old_package, identifier)?;
                println!("[OK] Archivos movidos de '{}' a '{}'", old_package, identifier);
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
            _ => println!("[INFO] Continuando sin regenerar; el build puede fallar."),
        }
    } else {
        println!("\nNo se encontró ningún paquete Java anterior bajo: {}", java_root.display());
        println!("Opciones:");
        println!("  1) Ejecutar 'tauri android init' (genera proyecto default)");
        println!("  2) Saltar (el build puede fallar)");
        print!("Elige [1/2]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        match input.trim() {
            "1" => {
                println!("[INFO] Ejecutando tauri android init...");
                if let Err(e) = run_tauri_android_init(ctx) {
                    println!("[ERROR] {}", e);
                } else {
                    println!("[OK] Proyecto Android regenerado.");
                }
            }
            _ => println!("[INFO] Continuando sin regenerar; el build puede fallar."),
        }
    }
    ui::pause();
    Ok(())
}

/// Recorre `root` buscando el primer directorio hoja que contenga `.kt`.
///
/// Un "directorio hoja de paquete" es el nivel más profundo con código Kotlin,
/// distinguido de los paquetes intermedios (`com/`, `com/keimer/`) que sólo
/// contienen subdirectorios.
fn find_kotlin_package(root: &std::path::Path) -> Option<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, depth: usize) -> Option<std::path::PathBuf> {
        if depth > 8 {
            return None;
        }
        let entries: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .filter(|p| {
                !p.file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(true)
            })
            .collect();

        // Si hay `.kt` aquí, este es el paquete.
        let has_kt = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .any(|e| e.path().extension().map(|x| x == "kt").unwrap_or(false));
        if has_kt {
            return Some(dir.to_path_buf());
        }

        entries.iter().find_map(|p| walk(p, depth + 1))
    }
    if root.is_dir() {
        walk(root, 0)
    } else {
        None
    }
}

/// Reconstruye el nombre de paquete (`com.keimer.app`) a partir de la ruta
/// relativa del directorio bajo `root` (`com/keimer/app`).
fn package_of_dir(root: &std::path::Path, dir: &std::path::Path) -> Option<String> {
    let rel = dir.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Mueve el paquete viejo al nuevo y reescribe las declaraciones `package`.
///
/// El reemplazo es del paquete completo viejo por el nuevo, en notación de
/// paquete, que es lo que debe aparecer tanto en `package com.x.y` como en las
/// referencias `com.x.y.Clase` del código. Se escribe el destino completo en vez
/// de parchear sobre el archivo original, para no dejar restos del anterior.
fn move_package(
    old_dir: &std::path::Path,
    new_dir: &std::path::Path,
    old_package_dotted: &str,
    new_package_dotted: &str,
) -> Result<()> {
    std::fs::create_dir_all(new_dir).context("No se pudo crear el directorio del paquete")?;

    for entry in std::fs::read_dir(old_dir).into_iter().flatten().filter_map(|e| e.ok()) {
        let file_name = entry.file_name();
        let dest = new_dir.join(&file_name);
        if file_name.to_string_lossy().ends_with(".kt") {
            let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
            // De nota de paquete a nota de paquete: `com.viejo.App` -> `com.nuevo.App`.
            let replaced = content.replace(old_package_dotted, new_package_dotted);
            std::fs::write(&dest, replaced)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
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

/// Acepta todas las licencias del SDK.
///
/// `sdkmanager --licenses` es interactivo y con `cmd::run` (que captura stdout y
/// no conecta stdin) se quedaba esperando o aceptaba cero licencias, haciendo
/// fallar la instalación posterior con un "license not accepted". Se alimenta
/// `y` por stdin, que es lo que acepta el prompt.
fn accept_licenses(sdk_manager: &std::path::Path) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let path = sdk_manager
        .to_str()
        .context("Ruta de sdkmanager inválida")?;

    let mut child = Command::new(path)
        .arg("--licenses")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            "No se pudo ejecutar sdkmanager --licenses.\n\
             Verifica que JAVA_HOME esté definido y que el JDK tenga java."
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        // 30 respuestas 'y' cubren de sobra el número de licencias pendientes.
        for _ in 0..30 {
            let _ = stdin.write_all(b"y\n");
            let _ = stdin.flush();
        }
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        anyhow::bail!(
            "sdkmanager --licenses falló: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
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

    accept_licenses(sdk_manager)?;
    let out = cmd::run(sdk_manager.to_str().unwrap(), &[&format!("ndk;{ndk_version}")])?;
    if !out.status.success() {
        anyhow::bail!(
            "No se pudo instalar el NDK {ndk_version}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

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
    // Detiene daemons de Java que bloquean los locks de build.
    //
    // `pkill -f java` era demasiado amplio: mata cualquier proceso Java del
    // usuario, incluidos otros proyectos en curso e IDEs como Android Studio o
    // IntelliJ. Se limita a los daemons de Gradle, que es lo que realmente
    // retiene los locks.
    if platform::IS_WINDOWS {
        let _ = cmd::run("taskkill", &["/F", "/IM", "java.exe", "/T"]);
    } else {
        let _ = cmd::run("pkill", &["-f", "GradleDaemon"]);
        // Kotlin daemon: también retiene caches de compilación.
        let _ = cmd::run("pkill", &["-f", "KotlinCompileDaemon"]);
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

    // Selecciona el dispositivo físico conectado.
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
        println!("  Conecta el teléfono y desbloquéalo, con la depuración USB activada");
        println!("  y la huella RSA aceptada. Verifica con: adb devices -l");
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

    // Windows abre Edge (que trae el inspector de WebView integrado con su
    // propio prefijo); en Linux se usa Chrome con `chrome://`, porque Edge no
    // registra ese esquema y `xdg-open` no sabría abrirlo.
    let (url, browser_hint) = if platform::IS_WINDOWS {
        ("edge://inspect/#devices", "Edge")
    } else {
        ("chrome://inspect/#devices", "Chrome")
    };
    println!();
    println!("[INFO] Abriendo {}://inspect/#devices ...", if platform::IS_WINDOWS { "edge" } else { "chrome" });

    if platform::IS_WINDOWS {
        let _ = cmd::run("powershell", &[
            "-Command", "Set-Clipboard", url
        ]);
        let _ = cmd::run("cmd", &["/c", "start", "msedge", url]);
    } else {
        // Copiar al portapapeles vía wl-copy/xclip/xsel (Wayland o X11) y abrir
        // con xdg-open. Ninguno es obligatorio: si no hay ninguno, se abre igual.
        let copiers: &[&[&str]] = &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "--clipboard", "--input"],
        ];
        let mut copied = false;
        for args in copiers {
            let mut argv: Vec<&str> = args.to_vec();
            argv.push(url);
            if let Ok(out) = cmd::run(args[0], &argv) {
                if out.status.success() {
                    copied = true;
                    break;
                }
            }
        }
        if !copied {
            println!("[WARN] Ningún copiador de portapapeles disponible (wl-copy/xclip/xsel).");
            println!("       Instala uno con: sudo apt install wl-clipboard");
        }
        let _ = cmd::run("xdg-open", &[url]);
    }
    let _ = browser_hint;

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
            println!("  4. En Linux, el navegador debe ser Chromium/Chrome: Firefox");
            println!("     y Brave no implementan el protocolo de WebView DevTools.");
        }
    }

    ui::pause();
    Ok(())
}

pub fn configure_signing(ctx: &ProjectContext) -> Result<()> {
    signer::create_or_import(ctx)?;
    Ok(())
}
