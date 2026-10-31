use anyhow::{Context, Result};
use std::env;
use std::io::{self, Write};

use crate::utils::{android as android_utils, cmd, context::ProjectContext, platform, tauri, ui};

fn ensure_android_project(ctx: &ProjectContext) -> Result<()> {
    if ctx.has_android_project {
        return Ok(());
    }
    println!("[WARNING] No se encontró el proyecto Android.");
    println!("[INFO] Inicializando proyecto Android con Tauri...");
    run_tauri_android_init(ctx)?;
    if platform::find_gradlew(&ctx.script_dir.join("src-tauri").join("gen").join("android"))
        .is_none()
    {
        anyhow::bail!("[ERROR] No se pudo inicializar Gradle en el proyecto Android.");
    }
    Ok(())
}

/// Corre `tauri android init` con el entry point del paquete.
///
/// Se invoca `node` + `tauri.js` en vez de `npx tauri` por lo mismo que
/// [`crate::utils::tauri::command`]: el shim de `node_modules/.bin` sólo existe
/// para el sistema que hizo el install, y `npx` además puede intentar
/// descargar el paquete de la red si no lo encuentra localmente.
fn run_tauri_android_init(ctx: &ProjectContext) -> Result<()> {
    let Some(entry) = tauri::cli_entry(ctx) else {
        anyhow::bail!(
            "No se encontró @tauri-apps/cli.\nEjecuta `npm install` en: {}",
            ctx.script_dir.display()
        );
    };
    let node = platform::exe_name("node");
    let entry = entry.to_string_lossy().into_owned();
    cmd::run_in_dir_status(&node, &[entry.as_str(), "android", "init"], &ctx.script_dir)
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
    let conf_str =
        std::fs::read_to_string(&conf_path).context("No se pudo leer tauri.conf.json")?;
    let conf: serde_json::Value =
        serde_json::from_str(&conf_str).context("tauri.conf.json no es JSON valido")?;
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

    let java_root = ctx
        .script_dir
        .join("src-tauri/gen/android/app/src/main/java");

    // Busca cualquier directorio hoja bajo `java/` que contenga archivos .kt.
    // Ése es el paquete generado que quedó con el identifier anterior.
    if let Some(old_leaf) = find_kotlin_package(&java_root) {
        // El paquete antiguo se reconstruye desde el propio directorio, no desde
        // un prefijo supuesto.
        let old_package = package_of_dir(&java_root, &old_leaf)
            .map(|p| p.replace('/', "."))
            .unwrap_or_else(|| {
                old_leaf
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });

        println!("\nSe detecto el paquete anterior: {}", old_package);
        println!("Opciones:");
        println!(
            "  1) Mover '{}' a '{}' y reescribir los 'package'",
            old_package, identifier
        );
        println!("  2) Ejecutar 'tauri android init' (sobreescribe archivos default)");
        println!("  3) Saltar (el build puede fallar)");
        print!("Elige [1/2/3]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        match input.trim() {
            "1" => {
                move_package(&old_leaf, &expected_dir, &old_package, identifier)?;
                println!(
                    "[OK] Archivos movidos de '{}' a '{}'",
                    old_package, identifier
                );
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
        println!(
            "\nNo se encontró ningún paquete Java anterior bajo: {}",
            java_root.display()
        );
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

    for entry in std::fs::read_dir(old_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
    {
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

    let missing: Vec<&str> = required
        .iter()
        .filter(|t| !installed.contains(*t))
        .copied()
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    println!(
        "[INFO] Instalando targets de Rust para Android: {}...",
        missing.join(", ")
    );
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

    let sdk_manager = ctx
        .sdk_manager
        .as_ref()
        .context("[ERROR] No se encontró sdkmanager. Verifica ANDROID_HOME.")?;

    let ndk_version = platform::NDK_VERSION;
    println!("[INFO] Instalando NDK {}...", ndk_version);

    accept_licenses(sdk_manager)?;
    let out = cmd::run(
        sdk_manager.to_str().unwrap(),
        &[&format!("ndk;{ndk_version}")],
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "No se pudo instalar el NDK {ndk_version}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let ndk_path = ctx
        .android_home
        .as_ref()
        .unwrap()
        .join("ndk")
        .join(ndk_version);
    env::set_var("ANDROID_NDK_HOME", &ndk_path);

    println!("[OK] NDK instalado.\n");
    Ok(())
}

fn cleanup_build_locks(ctx: &ProjectContext) {
    let cargo_lock = ctx
        .script_dir
        .join("src-tauri")
        .join("target")
        .join(".cargo-lock");
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
    // Sólo se detienen los procesos Java de Gradle; terminar todo Java también
    // afectaría otros proyectos e IDEs como Android Studio o IntelliJ.
    let _ = cmd::run("taskkill", &["/F", "/IM", "java.exe", "/T"]);
}

/// Variables que el Gradle de Tauri lee para fijar la ABI de un build.
///
/// Se pasan a `cmd::run_shell_status` en vez de mutar el entorno del proceso:
/// así un build individual no contamina los siguientes (antes `set_arch_*`
/// dejaba `ORG_GRADLE_PROJECT_*` puestas hasta el `clear_arch_env`).
fn arch_envs(target: &str) -> Vec<(String, String)> {
    let (abi, arch) = match target {
        "aarch64" => ("arm64-v8a", "arm64"),
        "armv7" => ("armeabi-v7a", "arm"),
        _ => return Vec::new(),
    };
    vec![
        ("ORG_GRADLE_PROJECT_abiList".to_string(), abi.to_string()),
        ("ORG_GRADLE_PROJECT_archList".to_string(), arch.to_string()),
        (
            "ORG_GRADLE_PROJECT_targetList".to_string(),
            target.to_string(),
        ),
    ]
}

/// Nombre del script de `package.json` con el que arrancar el dev server en
/// Android.
const NPM_DEV_ANDROID: &str = "dev:android";

/// El dev server de Tauri compila para arm64 por defecto.
///
/// Se fija en el entorno del proceso (y no con `arch_envs`) porque
/// `spawn_new_window_interactive` no acepta variables extra: el hijo las
/// hereda. Los builds release sí las pasan por `run_shell_status`.
fn set_arch_arm64() {
    env::set_var("ORG_GRADLE_PROJECT_abiList", "arm64-v8a");
    env::set_var("ORG_GRADLE_PROJECT_archList", "arm64");
    env::set_var("ORG_GRADLE_PROJECT_targetList", "aarch64");
}

/// Precondiciones compartidas por las dos rutas que arrancan la app en modo
/// debug en Android (las tarjetas «Android» y «Debug Andr»).
///
/// No incluye la selección de dispositivo: [`debug`] prefiere avisar con su
/// propio diagnóstico cuando no hay ninguno, en vez de abortar aquí.
fn prepare_android_dev(ctx: &ProjectContext) -> Result<()> {
    tauri::ensure(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;
    ensure_package_consistent(ctx)?;

    set_arch_arm64();
    cleanup_build_locks(ctx);
    ensure_ndk(ctx)?;
    ensure_rust_targets()?;
    Ok(())
}

/// ¿El proyecto declara `dev:android` entre los `scripts` de `package.json`?
fn declares_npm_dev_android(ctx: &ProjectContext) -> bool {
    let Ok(raw) = std::fs::read_to_string(ctx.script_dir.join("package.json")) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .map(|pkg| pkg["scripts"][NPM_DEV_ANDROID].is_string())
        .unwrap_or(false)
}

/// Cómo se lanza la app en modo debug en Android.
///
/// Se prefiere el script `dev:android` de `package.json` cuando el proyecto lo
/// declara: es el comando que su autor usa a mano, con sus propios flags
/// (`--host`, `beforeDevCommand` de Android…) y sus overrides de `tauri.*.conf.json`.
/// Reconstruirlo aquí como `tauri android dev` los pierde, que es justo lo que
/// dejaba el WebView cargando `localhost` sin túnel detrás. [`DevLaunch::Cli`]
/// queda sólo para proyectos que no declaren ese script.
#[derive(Debug, PartialEq, Eq)]
enum DevLaunch {
    /// El proyecto declara `dev:android`.
    NpmScript,
    /// No lo declara: se invoca el CLI de Tauri directamente.
    Cli,
}

impl DevLaunch {
    fn detect(ctx: &ProjectContext) -> Self {
        if declares_npm_dev_android(ctx) {
            Self::NpmScript
        } else {
            Self::Cli
        }
    }

    /// Comando listo para incrustar en [`cmd::spawn_new_window_interactive`].
    ///
    /// En Windows se cita `npm.cmd` y no `npm` a propósito, igual que en
    /// [`cmd::run_npm_install`]: el shim `npm.ps1` lo bloquea la
    /// ExecutionPolicy por defecto.
    fn command(&self, ctx: &ProjectContext) -> String {
        match self {
            Self::NpmScript => format!("npm.cmd run {NPM_DEV_ANDROID}"),
            Self::Cli => tauri::command(ctx, "android dev"),
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            Self::NpmScript => "script `dev:android` de package.json",
            Self::Cli => "CLI de Tauri (package.json no declara `dev:android`)",
        }
    }
}

pub fn dev(ctx: &ProjectContext) -> Result<()> {
    prepare_android_dev(ctx)?;

    // Selecciona el dispositivo físico conectado.
    let _device = android_utils::select_device(ctx)?;

    ui::info_banner("Iniciando desarrollo Android");
    println!("  Tauri se conectará automáticamente al dispositivo disponible.");
    println!();

    let launch = DevLaunch::detect(ctx);
    println!("[INFO] Arrancando con el {}.", launch.describe());

    cmd::spawn_new_window_interactive("Tauri Android Dev", &ctx.script_dir, &launch.command(ctx))?;

    Ok(())
}

/// Precondiciones comunes de los builds release de Android.
fn prepare_release(ctx: &ProjectContext) -> Result<()> {
    tauri::ensure(ctx)?;
    ctx.assert_android_sdk()?;
    ensure_android_project(ctx)?;
    ensure_package_consistent(ctx)?;
    Ok(())
}

fn apk_output_dir(ctx: &ProjectContext) -> std::path::PathBuf {
    ctx.script_dir
        .join("src-tauri/gen/android/app/build/outputs/apk")
}

/// `<Product>[-v<ver>][-<label>]` con el nombre saneado del APK.
fn apk_artifact_base(ctx: &ProjectContext, label: Option<&str>) -> String {
    let (name, version) = read_tauri_app_meta(ctx).unwrap_or_else(|| {
        (
            sanitize_app_name(&platform::dir_display_name(&ctx.script_dir)),
            String::new(),
        )
    });
    match (version.is_empty(), label) {
        (true, None) => name,
        (true, Some(l)) => format!("{name}-{l}"),
        (false, None) => format!("{name}-v{version}"),
        (false, Some(l)) => format!("{name}-v{version}-{l}"),
    }
}

fn newest_apk(ctx: &ProjectContext) -> Result<std::path::PathBuf> {
    let dir = apk_output_dir(ctx);
    cmd::newest_artifact(&dir, &["apk"]).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] El build termino bien pero no se encontro la APK en: {}",
            dir.display()
        )
    })
}

/// Compila el APK release de **una** arquitectura (`aarch64` o `armv7`).
pub fn build_arch(ctx: &ProjectContext, target: &str, label: &str) -> Result<std::path::PathBuf> {
    prepare_release(ctx)?;

    let command = tauri::command(ctx, &format!("android build --target {target} --apk"));
    println!();
    println!("[INFO] Compilando Android APK {label} (Release)...");
    println!("[INFO] Usando: {command}");
    println!();

    if !cmd::run_shell_status(&command, &ctx.script_dir, &arch_envs(target))? {
        anyhow::bail!(
            "[ERROR] Fallo el build Android {label} (`tauri android build --target {target}`)."
        );
    }

    let apk = newest_apk(ctx)?;
    let base = apk_artifact_base(ctx, Some(label));
    cmd::rename_artifact(&apk, &base)
        .ok_or_else(|| anyhow::anyhow!("[ERROR] No se pudo renombrar {}", apk.display()))
}

/// APK release **universal**: una sola APK con todas las ABIs del proyecto.
pub fn build_universal(ctx: &ProjectContext) -> Result<std::path::PathBuf> {
    prepare_release(ctx)?;

    let command = tauri::command(ctx, "android build --apk");
    println!();
    println!("[INFO] Compilando Android APK universal (Release)...");
    println!("[INFO] Usando: {command}");
    println!();

    if !cmd::run_shell_status(&command, &ctx.script_dir, &[])? {
        anyhow::bail!("[ERROR] Fallo el build Android universal (`tauri android build --apk`).");
    }

    let apk = newest_apk(ctx)?;
    let base = apk_artifact_base(ctx, None);
    cmd::rename_artifact(&apk, &base)
        .ok_or_else(|| anyhow::anyhow!("[ERROR] No se pudo renombrar {}", apk.display()))
}

/// AAB release para Play Store.
pub fn build_aab(ctx: &ProjectContext) -> Result<()> {
    let aab = build_aab_artifact(ctx)?;
    cmd::show_artifact(&aab);
    ui::pause_for(10);
    Ok(())
}

fn build_aab_artifact(ctx: &ProjectContext) -> Result<std::path::PathBuf> {
    prepare_release(ctx)?;

    let command = tauri::command(ctx, "android build --aab");
    println!();
    println!("[INFO] Compilando Android AAB (Release)...");
    println!("[INFO] Usando: {command}");
    println!();

    if !cmd::run_shell_status(&command, &ctx.script_dir, &[])? {
        anyhow::bail!("[ERROR] Fallo el build Android AAB (`tauri android build --aab`).");
    }

    let dir = ctx
        .script_dir
        .join("src-tauri/gen/android/app/build/outputs/bundle");
    let aab = cmd::newest_artifact(&dir, &["aab"]).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] El build termino bien pero no se encontro el AAB en: {}",
            dir.display()
        )
    })?;
    let base = apk_artifact_base(ctx, None);
    cmd::rename_artifact(&aab, &base)
        .ok_or_else(|| anyhow::anyhow!("[ERROR] No se pudo renombrar {}", aab.display()))
}

pub fn build_arm64(ctx: &ProjectContext) -> Result<()> {
    let apk = build_arch(ctx, "aarch64", "arm64")?;
    cmd::show_artifact(&apk);
    ui::pause_for(10);
    Ok(())
}

pub fn build_armv7(ctx: &ProjectContext) -> Result<()> {
    let apk = build_arch(ctx, "armv7", "armv7")?;
    cmd::show_artifact(&apk);
    ui::pause_for(10);
    Ok(())
}

pub fn build_all(ctx: &ProjectContext) -> Result<()> {
    let apk = build_universal(ctx)?;
    cmd::show_artifact(&apk);
    ui::pause_for(10);
    Ok(())
}

pub fn debug(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::info_banner("Depurador Chrome (Android)");

    // Preparar el entorno ANTES de tocar adb: `tauri::ensure` puede acabar
    // ejecutando `npm install`, y el `kill-server` de más abajo tumbaría ese
    // proceso a media instalación.
    prepare_android_dev(ctx)?;

    let adb = ctx
        .adb_path
        .as_ref()
        .context("[ERROR] ADB no encontrado. Verifica ANDROID_HOME.")?;
    let adb_str = adb.to_str().unwrap();

    println!("[1/5] Reiniciando servidor ADB...");
    cmd::run(adb_str, &["kill-server"]).ok();
    std::thread::sleep(std::time::Duration::from_secs(1));
    cmd::run(adb_str, &["start-server"])?;
    std::thread::sleep(std::time::Duration::from_secs(2));

    println!("[2/5] Buscando dispositivos...");
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
        println!("[3/5] Abriendo puerto de depuración...");
        for line in stdout.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.contains("offline") || !trimmed.contains("device") {
                continue;
            }
            let serial = trimmed.split_whitespace().next().unwrap_or("");
            if !serial.is_empty() {
                let _ = cmd::run(
                    adb_str,
                    &[
                        "-s",
                        serial,
                        "forward",
                        "tcp:9222",
                        "localabstract:chrome_devtools_remote",
                    ],
                );
                let _ = cmd::run(
                    adb_str,
                    &[
                        "-s",
                        serial,
                        "forward",
                        "tcp:9229",
                        "localabstract:firefox_devtools_remote",
                    ],
                );
            }
        }
    }

    // Lanza la app DESPUÉS del `kill-server` del paso 1, nunca antes: es el
    // `beforeDevCommand` del script `dev:android` quien levanta el `adb reverse`
    // del puerto del dev server. Lanzarla antes dejaría al `kill-server`
    // borrando esos túneles y el WebView volvería a pedir `localhost` sin nada
    // detrás: el "connection refused" con la página en blanco.
    println!("[4/5] Lanzando la app en modo debug...");
    let launch = DevLaunch::detect(ctx);
    println!("       Con el {}.", launch.describe());
    if let Err(e) = cmd::spawn_new_window_interactive(
        "Tauri Android Dev",
        &ctx.script_dir,
        &launch.command(ctx),
    ) {
        println!("[WARN] No se pudo lanzar la ventana de dev: {e}");
    }
    println!();

    println!("[5/5] Abriendo Chrome DevTools...");

    let url = "edge://inspect/#devices";
    println!();
    println!("[INFO] Abriendo edge://inspect/#devices ...");
    // `Set-Clipboard` y `Start-Process` existen en PowerShell 5.1 y 7, así que
    // no hace falta el `clip`/`start` de cmd.
    let _ = cmd::run_ps(&format!("Set-Clipboard -Value {}", cmd::ps_quote(url)));
    let _ = cmd::run_ps(&format!(
        "Start-Process -FilePath msedge -ArgumentList {}",
        cmd::ps_quote(url)
    ));

    if has_device {
        println!("[INFO] Si no ves el dispositivo en Edge, verifica:");
        println!("  1. En edge://inspect, haz clic en 'Discover USB devices'");
        println!("  2. O usa: adb forward tcp:9222 localabstract:chrome_devtools_remote");
        println!("  3. Abre http://localhost:9222/json en Edge");
    }

    ui::pause();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::context::ProjectType;

    /// Contexto de mentira sobre un directorio temporal. Sin `tempfile` en las
    /// dev-dependencies: cada test crea lo suyo y lo borra, con el PID en el
    /// nombre para que no se pisen entre sí.
    fn fake_ctx(name: &str) -> (std::path::PathBuf, ProjectContext) {
        let root = std::env::temp_dir().join(format!("cr-android-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ctx = ProjectContext {
            script_dir: root.clone(),
            project_type: ProjectType::Tauri,
            android_home: None,
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        };
        (root, ctx)
    }

    fn write_pkg(root: &std::path::Path, body: &str) {
        std::fs::write(root.join("package.json"), body).unwrap();
    }

    #[test]
    fn uses_the_npm_script_when_the_project_declares_it() {
        let (root, ctx) = fake_ctx("npm-script");
        write_pkg(
            &root,
            r#"{ "scripts": { "dev:android": "tauri android dev --host 127.0.0.1" } }"#,
        );

        assert_eq!(DevLaunch::detect(&ctx), DevLaunch::NpmScript);

        let cmd = DevLaunch::NpmScript.command(&ctx);
        assert_eq!(cmd, "npm.cmd run dev:android");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Un proyecto sin `dev:android` no puede romperse: cae al CLI de Tauri.
    #[test]
    fn falls_back_to_the_cli_without_the_script() {
        let (root, ctx) = fake_ctx("no-script");
        write_pkg(
            &root,
            r#"{ "scripts": { "dev": "vite", "build": "vite build" } }"#,
        );

        assert_eq!(DevLaunch::detect(&ctx), DevLaunch::Cli);
        assert_eq!(DevLaunch::Cli.command(&ctx), "npx.cmd tauri android dev");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Sin `package.json`, o con JSON inválido, tampoco debe abortar: la caída
    /// al CLI es el resultado correcto, no un error.
    #[test]
    fn a_missing_or_broken_package_json_falls_back_quietly() {
        let (root, ctx) = fake_ctx("broken-pkg");
        assert_eq!(DevLaunch::detect(&ctx), DevLaunch::Cli);

        write_pkg(&root, "{ esto no es json");
        assert_eq!(DevLaunch::detect(&ctx), DevLaunch::Cli);

        // Y con `scripts` presente pero vacío: que no entre en pánico.
        write_pkg(&root, r#"{ "scripts": {} }"#);
        assert_eq!(DevLaunch::detect(&ctx), DevLaunch::Cli);

        let _ = std::fs::remove_dir_all(&root);
    }
}
