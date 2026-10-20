//! Soporte de **Go + Wails v3**.
//!
//! Un proyecto Wails v3 se reconoce por `go.mod` con `wailsapp/wails` y un
//! `Taskfile.yml` en la raíz (la v2 usaba `wails.json` y otro CLI, así que no
//! se mezcla: ver `context::is_wails_v3`).
//!
//! ## Por qué el build de Android **no** usa el Taskfile del proyecto
//!
//! Las tareas `android:*` de la plantilla de Wails v3 están escritas en POSIX
//! (`uname -s`, `ls -d`, `sort -V`, `chmod +x`) y su `case` de host sólo
//! contempla Darwin y Linux:
//!
//! ```sh
//! case "$(uname -s)" in
//!   Darwin) HOST_TAG="darwin-x86_64" ;;
//!   Linux)  HOST_TAG="linux-x86_64" ;;
//!   *)      echo "Unsupported host OS"; exit 1 ;;
//! esac
//! ```
//!
//! En Windows el `case` cae en `*)` y el build aborta, aunque la NDK del host
//! sí exista (`windows-x86_64`). Por eso Compass hace el pipeline de Android
//! por su cuenta, replicando los pasos del Taskfile pero con el *host tag* y
//! el compilador correctos para cada plataforma:
//!
//! 1. `wails3 android overlay:gen` → `build/android/overlay.json`
//! 2. `wails3 generate bindings` (con `GOOS=android`, `CGO_ENABLED=0`)
//! 3. Frontend: `npm install` + `npm run build` en `frontend/`
//! 4. `go build -buildmode=c-shared` → `libwails.so` con la NDK
//! 5. Gradle: `assembleDebug` / `assembleRelease` en `build/android/`
//!
//! El **escritorio** sí delega en el Taskfile (`wails3 task build`), porque sus
//! tareas ya son multiplataforma (Go cross-compila solo).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::utils::{cmd, context::ProjectContext, platform, ui};
use crate::utils::android as android_utils;

/// `minSdk` de la plantilla de Wails v3 (`build/android/app/build.gradle`).
/// De ahí sale el sufijo del compilador de la NDK: `aarch64-linux-android21-clang`.
const MIN_SDK: u32 = 21;

/// Arquitectura de la APK que se compila: `arm64` cubre el 99% de los
/// teléfonos reales (`x86_64` es sólo para emuladores).
const ARCH: &str = "arm64";

/// Carpeta ABI de la NDK para `arm64`.
const JNI_DIR: &str = "arm64-v8a";

// ───────────────────────────────────────────────────────────────────────────
// Configuración del proyecto
// ───────────────────────────────────────────────────────────────────────────

/// Lee una variable del bloque `vars:` del `Taskfile.yml` de la raíz.
///
/// Sólo se aceptan **literales** (`APP_NAME: "school-tracker"`). Los valores
/// con plantilla (`'{{.FOO | default "x"}}'`) se ignoran: dependen del task, no
/// del proyecto.
fn taskfile_var(ctx: &ProjectContext, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(ctx.script_dir.join("Taskfile.yml")).ok()?;
    for line in text.lines() {
        // Las vars de la raíz van indentadas con 2 espacios; las de los
        // Taskfiles incluidos viven en otros ficheros.
        let indent = line.len() - line.trim_start().len();
        if indent == 0 || indent > 4 {
            continue;
        }
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(&format!("{name}:")) else {
            continue;
        };
        let value = rest.trim();
        if value.starts_with('"') {
            let end = value[1..].find('"')? + 1;
            return Some(value[1..end].to_string());
        }
        if value.starts_with('\'') {
            let end = value[1..].find('\'')? + 1;
            return Some(value[1..end].to_string());
        }
    }
    None
}

/// Nombre de la aplicación (`APP_NAME` del Taskfile; si no, la carpeta).
fn app_name(ctx: &ProjectContext) -> String {
    taskfile_var(ctx, "APP_NAME").unwrap_or_else(|| {
        ctx.script_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "app".to_string())
    })
}

/// Carpeta donde el Taskfile deja los binarios (`BIN_DIR`; por defecto `bin/`).
fn bin_dir(ctx: &ProjectContext) -> PathBuf {
    let dir = taskfile_var(ctx, "BIN_DIR").unwrap_or_else(|| "bin".to_string());
    ctx.script_dir.join(dir)
}

/// Carpeta del proyecto Android de Wails (`build/android`).
fn android_dir(ctx: &ProjectContext) -> PathBuf {
    ctx.script_dir.join("build").join("android")
}

/// `applicationId` del módulo Android (`build/android/app/build.gradle`).
///
/// La plantilla de Wails usa `com.wails.app`, pero el proyecto puede
/// cambiarlo; el `namespace` (con el que se firma la actividad) es otro valor
/// distinto, y por eso se leen los dos.
fn application_id(ctx: &ProjectContext) -> String {
    let gradle = android_dir(ctx).join("app").join("build.gradle");
    for path in [gradle.clone(), gradle.with_file_name("build.gradle.kts")] {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("applicationId") {
                let v = rest.trim().trim_matches('"').trim_matches('\'').trim();
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    "com.wails.app".to_string()
}

/// Clase completa de la actividad principal (`com.wails.app.MainActivity`).
///
/// En la plantilla el `namespace` del módulo Android es `com.wails.app` y la
/// actividad se declara como `.MainActivity`, así que la clase completa es
/// `<namespace>.MainActivity` (independiente del `applicationId`).
fn main_activity(ctx: &ProjectContext) -> String {
    let gradle = android_dir(ctx).join("app").join("build.gradle");
    for path in [gradle.clone(), gradle.with_file_name("build.gradle.kts")] {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("namespace") {
                let v = rest.trim().trim_matches('"').trim_matches('\'').trim();
                if !v.is_empty() {
                    return format!("{v}.MainActivity");
                }
            }
        }
    }
    "com.wails.app.MainActivity".to_string()
}

// ───────────────────────────────────────────────────────────────────────────
// Utilidades
// ───────────────────────────────────────────────────────────────────────────

/// ¿Está `name` disponible como comando?
fn command_exists(name: &str) -> bool {
    #[cfg(windows)]
    {
        let ps = format!(
            "$null -ne (Get-Command -Name '{name}' -ErrorAction SilentlyContinue)"
        );
        cmd::run("powershell", &["-NoProfile", "-Command", &ps])
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True")
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("sh")
            .args(["-c", &format!("command -v -- {name}")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Comprueba que estén el CLI de Wails y Go.
fn ensure_wails_cli() -> Result<()> {
    if !command_exists("wails3") {
        anyhow::bail!(
            "[ERROR] No se encontró el CLI `wails3`.\n\
             Instálalo con:\n  go install github.com/wailsapp/wails/v3/cmd/wails3@latest\n\
             (el binario queda en $(go env GOPATH)/bin, que debe estar en el PATH)."
        );
    }
    if !command_exists("go") {
        anyhow::bail!(
            "[ERROR] No se encontró Go en el PATH.\n\
             Instálalo desde https://go.dev/dl/ y reinstala el CLI de Wails."
        );
    }
    Ok(())
}

/// NDK: `ANDROID_NDK_HOME` → la de `ctx` → la más reciente bajo `$ANDROID_HOME/ndk`.
fn ndk_root(ctx: &ProjectContext) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("ANDROID_NDK_HOME") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    if let Some(ndk) = &ctx.ndk_home {
        if ndk.is_dir() {
            return Some(ndk.clone());
        }
    }
    // La más reciente por número de versión (26.1 > 9.0).
    let sdk = ctx.android_home.as_ref()?;
    let mut versions: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(sdk.join("ndk"))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let key: Vec<u64> = name.split('.').filter_map(|p| p.parse().ok()).collect();
            if key.is_empty() {
                return None;
            }
            Some((key, e.path()))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().next().map(|(_, p)| p)
}

/// Carpeta de la toolchain LLVM de la NDK para el host actual.
fn ndk_toolchain(ndk: &Path) -> Option<PathBuf> {
    let prebuilt = ndk.join("toolchains").join("llvm").join("prebuilt");
    // El NDK de Windows es `windows-x86_64`; en Linux `linux-x86_64` y en
    // macOS `darwin-x86_64` (universal). Se busca por existencia, no por
    // plataforma, para no depender de `cfg!`.
    for tag in ["windows-x86_64", "linux-x86_64", "darwin-x86_64"] {
        let bin = prebuilt.join(tag).join("bin");
        if bin.is_dir() {
            return Some(bin);
        }
    }
    None
}

/// Compilador C/C++ de la NDK para la arquitectura objetivo.
///
/// En Windows el NDK trae un wrapper `.cmd` (y un script bash sin extensión,
/// que no es ejecutable ahí): se llama a `clang.exe` **con el `--target`
/// explícito**, que es lo que hace el wrapper.
fn ndk_compilers(bin: &Path) -> Option<(String, String)> {
    let clang = bin.join("clang.exe");
    if clang.is_file() {
        let target = format!("aarch64-linux-android{MIN_SDK}");
        return Some((
            format!("{} --target={}", clang.display(), target),
            format!("{} --target={}", bin.join("clang++.exe").display(), target),
        ));
    }
    let cc = bin.join(format!("aarch64-linux-android{MIN_SDK}-clang"));
    let cxx = bin.join(format!("aarch64-linux-android{MIN_SDK}-clang++"));
    if cc.is_file() {
        return Some((cc.display().to_string(), cxx.display().to_string()));
    }
    None
}

/// Ejecuta un comando en el directorio dado, heredando la salida.
fn run(command: &str, dir: &Path, envs: &[(String, String)]) -> Result<bool> {
    cmd::run_shell_status(command, dir, envs)
}

/// `npm run <script>` en el frontend.
///
/// En Windows `npm` es `npm.cmd`, que `cmd /c` sí sabe ejecutar.
fn npm_run(dir: &Path, script: &str) -> Result<()> {
    let command = if platform::IS_WINDOWS {
        format!("npm.cmd run {script}")
    } else {
        format!("npm run {script}")
    };
    if !run(&command, dir, &[])? {
        anyhow::bail!("`npm run {script}` falló en: {}", dir.display());
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// Escritorio
// ───────────────────────────────────────────────────────────────────────────

/// Modo desarrollo de escritorio (`wails3 task dev`).
///
/// Levanta el Vite en modo dev y recompila el binario al vuelo, así que va en
/// una ventana interactiva y no en la ventana del menú.
pub fn dev(ctx: &ProjectContext) -> Result<()> {
    ensure_wails_cli()?;

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║          WAILS v3 - DEV (ESCRITORIO)             ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("[INFO] Comando: wails3 task dev");
    println!("[INFO] El servidor de Vite y la app se lanzan en la ventana nueva.");
    println!();

    cmd::spawn_new_window_interactive(
        "Wails Dev",
        &ctx.script_dir,
        "wails3 task dev",
    )?;

    Ok(())
}

/// Compila el binario de escritorio (`wails3 task build`) y lo revela.
///
/// El Taskfile de la raíz despacha a `windows:build` / `linux:build` según el
/// host, así que el mismo comando vale para los dos sistemas.
pub fn build_desktop(ctx: &ProjectContext) -> Result<()> {
    ensure_wails_cli()?;

    println!("\n[INFO] Compilando aplicacion de escritorio con Wails...\n");

    let bin = bin_dir(ctx);
    // En Windows el binario es `.exe`; en Linux no lleva extensión, y la lista
    // vacía significa "cualquier archivo" (ver `newest_artifact`).
    let exts: &[&str] = if platform::IS_WINDOWS { &["exe"] } else { &[] };

    let outcome = cmd::run_build_and_reveal_with_env_outcome(
        "wails3 task build",
        &ctx.script_dir,
        &bin,
        exts,
        None,
        &[],
    )?;

    if outcome.success && outcome.artifact.is_none() {
        println!("[WARN] No se encontro el binario en: {}", bin.display());
    }

    Ok(())
}

/// Empaqueta el instalador de Windows (NSIS/MSIX): `wails3 task package`.
///
/// Necesita `makensis` (NSIS) o las herramientas de MSIX; por eso es una
/// opción aparte del build, no la misma tarjeta.
pub fn package_desktop(ctx: &ProjectContext) -> Result<()> {
    ensure_wails_cli()?;

    println!("\n[INFO] Empaquetando instalador (wails3 task package)...\n");
    println!("[INFO] NSIS requiere `makensis` en el PATH; MSIX, `wails3 tool msix-install-tools`.");
    println!();

    let bin = bin_dir(ctx);
    let exts: &[&str] = if platform::IS_WINDOWS { &["exe", "msix"] } else { &[] };
    cmd::run_build_and_reveal("wails3 task package", &ctx.script_dir, &bin, exts, None)?;

    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// Android
// ───────────────────────────────────────────────────────────────────────────

/// Compila el APK (debug o release) con el pipeline nativo de Compass.
///
/// Devuelve la ruta del APK generado.
fn build_android_apk(ctx: &ProjectContext, release: bool) -> Result<PathBuf> {
    ensure_wails_cli()?;

    let android = android_dir(ctx);
    if !android.join("settings.gradle").exists() && !android.join("settings.gradle.kts").exists() {
        anyhow::bail!(
            "[ERROR] No se encontro el proyecto Android de Wails en: {}\n\
             Regenera los assets de build con: wails3 update build-assets",
            android.display()
        );
    }

    // ── 0. NDK ──
    let ndk = ndk_root(ctx).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] No se encontro la NDK de Android.\n\
             Instala una con: sdkmanager \"ndk;27.0.12077973\"\n\
             o define ANDROID_NDK_HOME."
        )
    })?;
    let toolchain = ndk_toolchain(&ndk).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] La NDK de {} no tiene la toolchain LLVM para este host.",
            ndk.display()
        )
    })?;
    let (cc, cxx) = ndk_compilers(&toolchain).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] No se encontro el compilador aarch64-linux-android{} en: {}",
            MIN_SDK,
            toolchain.display()
        )
    })?;

    let root = &ctx.script_dir;

    // ── 1. Overlay: registra las fuentes específicas de Android ──
    println!("[1/5] Generando el overlay de Android...");
    let overlay = android.join("overlay.json");
    let cmd_overlay = format!(
        "wails3 android overlay:gen -out \"{}\" -config build/config.yml",
        overlay.display()
    );
    if !run(&cmd_overlay, root, &[])? {
        anyhow::bail!("[ERROR] Fallo `wails3 android overlay:gen`.");
    }

    // ── 2. Bindings para el frontend ──
    println!("[2/5] Generando los bindings...");
    let tags = if release {
        "production,android"
    } else {
        "android,debug"
    };
    let cmd_bindings = format!("wails3 generate bindings -f \"-tags {tags}\" -clean=true -ts -i");
    let envs = vec![
        ("GOOS".to_string(), "android".to_string()),
        ("CGO_ENABLED".to_string(), "0".to_string()),
    ];
    if !run(&cmd_bindings, root, &envs)? {
        println!("[WARN] La generacion de bindings fallo; se continua con los que hubiera.");
    }

    // ── 3. Frontend ──
    println!("[3/5] Compilando el frontend...");
    let frontend = root.join("frontend");
    if frontend.join("package.json").exists() {
        if !frontend.join("node_modules").exists() {
            cmd::run_npm_install(&frontend)?;
        }
        npm_run(&frontend, "build")?;
    } else {
        println!("[WARN] No hay frontend/ con package.json; se sigue sin frontend.");
    }

    // ── 4. La librería compartida de Go (libwails.so) ──
    println!("[4/5] Compilando libwails.so con Go + NDK...");
    let jni_dir = android.join("app").join("src").join("main").join("jniLibs").join(JNI_DIR);
    std::fs::create_dir_all(&jni_dir)
        .with_context(|| format!("No se pudo crear: {}", jni_dir.display()))?;
    let so_path = jni_dir.join("libwails.so");

    let build_flags = if release {
        "-tags production,android -trimpath -buildvcs=false -ldflags=\"-w -s\""
    } else {
        "-tags android,debug -buildvcs=false -gcflags=all=\"-l\""
    };
    let cmd_go = format!(
        "go build -buildmode=c-shared -overlay \"{}\" {} -o \"{}\"",
        overlay.display(),
        build_flags,
        so_path.display()
    );
    let envs = vec![
        ("GOOS".to_string(), "android".to_string()),
        ("GOARCH".to_string(), ARCH.to_string()),
        ("CGO_ENABLED".to_string(), "1".to_string()),
        ("CC".to_string(), cc.clone()),
        ("CXX".to_string(), cxx.clone()),
    ];
    if !run(&cmd_go, root, &envs)? {
        anyhow::bail!(
            "[ERROR] Fallo la compilacion de libwails.so.\n\
             Revisa que la NDK sea compatible y que GOOS/GOARCH/CGO_ENABLED sean correctos."
        );
    }

    // ── 5. Gradle ──
    println!("[5/5] Ensamblando la APK con Gradle...");
    ensure_sdk_properties(ctx, &android)?;

    let gradle_task = if release { "assembleRelease" } else { "assembleDebug" };
    let wrapper = platform::find_gradlew(&android).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] No se encontro el wrapper de Gradle en: {}\n\
             Regenera los assets de build con: wails3 update build-assets",
            android.display()
        )
    })?;
    let gradle_cmd = format!("\"{}\" {}", wrapper.display(), gradle_task);
    if !run(&gradle_cmd, &android, &[])? {
        anyhow::bail!("[ERROR] Gradle falló en `{}`.", gradle_task);
    }

    // La APK vive en `app/build/outputs/apk/<variante>/`.
    let variant = if release { "release" } else { "debug" };
    let apk = android
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
        .join(variant);
    let apk_file = newest_in(&apk, "apk").ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] Gradle terminó bien pero no se encontró la APK en: {}",
            apk.display()
        )
    })?;

    // Copia a `bin/`, como hace el Taskfile del proyecto.
    let bin = bin_dir(ctx);
    std::fs::create_dir_all(&bin).ok();
    let dest = bin.join(format!("{}.apk", app_name(ctx)));
    if let Err(e) = std::fs::copy(&apk_file, &dest) {
        println!("[WARN] No se pudo copiar la APK a {}: {}", dest.display(), e);
    }

    Ok(apk_file)
}

/// Escribe `build/android/local.properties` con `sdk.dir`.
///
/// Gradle acepta `ANDROID_HOME`, pero no siempre está definido (sobre todo en
/// Windows, donde el SDK suele vivir en una ruta por usuario). El fichero es
/// local y lo regenera `wails3 update build-assets`.
fn ensure_sdk_properties(ctx: &ProjectContext, android: &Path) -> Result<()> {
    if std::env::var("ANDROID_HOME").is_ok() || std::env::var("ANDROID_SDK_ROOT").is_ok() {
        return Ok(());
    }
    let Some(sdk) = &ctx.android_home else {
        return Ok(());
    };
    let props = android.join("local.properties");
    let content = format!("sdk.dir={}\n", sdk.display());
    let current = std::fs::read_to_string(&props).unwrap_or_default();
    if current.contains(&content) {
        return Ok(());
    }
    std::fs::write(&props, content)
        .with_context(|| format!("No se pudo escribir: {}", props.display()))?;
    Ok(())
}

/// APK más reciente de una carpeta (recursivo).
fn newest_in(dir: &Path, ext: &str) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    fn walk(dir: &Path, ext: &str, best: &mut Option<(std::time::SystemTime, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, ext, best);
                continue;
            }
            let matches = path
                .extension()
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false);
            if !matches {
                continue;
            }
            if let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) {
                if best.as_ref().map_or(true, |(t, _)| mtime > *t) {
                    *best = Some((mtime, path));
                }
            }
        }
    }
    if dir.is_dir() {
        walk(dir, ext, &mut best);
    }
    best.map(|(_, p)| p)
}

/// APK **release** + opción de instalarlo en el teléfono.
pub fn build_apk(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Release) con Wails...\n");
    let apk = build_android_apk(ctx, true)?;

    let install = inquire::Confirm::new("¿Instalar el APK release en el dispositivo?")
        .with_default(true)
        .prompt()?;
    if install {
        install_apk(ctx, &apk)?;
    }

    Ok(())
}

/// APK **debug** + instalación y arranque en el teléfono.
pub fn install_debug(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Debug) con Wails...\n");
    let apk = build_android_apk(ctx, false)?;
    install_apk(ctx, &apk)?;
    Ok(())
}

/// AAB **release** para Play Store.
pub fn build_aab(ctx: &ProjectContext) -> Result<()> {
    ensure_wails_cli()?;

    println!("\n[INFO] Compilando Android AAB (Release) con Wails...\n");

    // El AAB es el mismo paquete que la APK, pero en formato bundle: se
    // compila la APK (que genera `libwails.so` y el frontend) y luego Gradle
    // empaqueta el bundle.
    let android = android_dir(ctx);
    ensure_sdk_properties(ctx, &android)?;

    let wrapper = platform::find_gradlew(&android).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] No se encontro el wrapper de Gradle en: {}",
            android.display()
        )
    })?;

    // Reutiliza el pipeline de la APK para tener `libwails.so` y el overlay.
    build_android_apk(ctx, true)?;

    let gradle_cmd = format!("\"{}\" bundleRelease", wrapper.display());
    if !run(&gradle_cmd, &android, &[])? {
        anyhow::bail!("[ERROR] Gradle falló en `bundleRelease`.");
    }

    let aab_dir = android
        .join("app")
        .join("build")
        .join("outputs")
        .join("bundle")
        .join("release");
    let aab = newest_in(&aab_dir, "aab").ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] Gradle terminó bien pero no se encontró el AAB en: {}",
            aab_dir.display()
        )
    })?;

    let bin = bin_dir(ctx);
    std::fs::create_dir_all(&bin).ok();
    let dest = bin.join(format!("{}.aab", app_name(ctx)));
    if let Err(e) = std::fs::copy(&aab, &dest) {
        println!("[WARN] No se pudo copiar el AAB a {}: {}", dest.display(), e);
    }

    println!();
    println!("[OK] AAB generado: {}", dest.display());
    println!("[INFO] Un AAB no se instala directamente en el telefono; es para Play Store.");

    Ok(())
}

/// Instala un APK ya compilado en el teléfono y abre la app.
fn install_apk(ctx: &ProjectContext, apk: &Path) -> Result<()> {
    let package = application_id(ctx);
    let activity = main_activity(ctx);
    android_utils::install_apk(ctx, apk, &package, &activity)
}

/// Logcat de la app en el dispositivo (sólo su PID, sin `grep`).
pub fn debug_android(ctx: &ProjectContext) -> Result<()> {
    let package = application_id(ctx);
    println!("\n[INFO] Depurando {} en el dispositivo...\n", package);

    let device = android_utils::select_device(ctx)?;
    let adb = ctx
        .adb_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "adb".to_string());

    // El PID cambia en cada arranque, así que se resuelve en el momento.
    let pid = cmd::run(
        &adb,
        &["-s", &device.serial, "shell", "pidof", &package],
    )
    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    .unwrap_or_default();

    let command = if pid.is_empty() {
        format!(
            "\"{adb}\" -s {} logcat -v time AndroidRuntime:E",
            device.serial
        )
    } else {
        format!("\"{adb}\" -s {} logcat -v time --pid={}", device.serial, pid)
    };

    cmd::spawn_new_window_interactive("Wails - logcat", &ctx.script_dir, &command)?;
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────────
// Limpieza
// ───────────────────────────────────────────────────────────────────────────

/// Qué se puede limpiar en un proyecto Go + Wails.
pub fn clean_targets(ctx: &ProjectContext) -> Vec<(String, PathBuf)> {
    let root = &ctx.script_dir;
    let android = android_dir(ctx);
    vec![
        ("bin/".into(), bin_dir(ctx)),
        ("frontend/dist".into(), root.join("frontend").join("dist")),
        ("frontend/bindings".into(), root.join("frontend").join("bindings")),
        (
            "Android app/build".into(),
            android.join("app").join("build"),
        ),
        ("Android .gradle".into(), android.join(".gradle")),
        (
            "Android jniLibs".into(),
            android.join("app").join("src").join("main").join("jniLibs"),
        ),
    ]
}

/// Limpieza: borra los artefactos de build (conserva las dependencias).
pub fn clean(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    ui::box_banner("LIMPIEZA");
    println!();
    println!("Se eliminará:");
    for (label, _) in clean_targets(ctx) {
        println!("  - {}", label);
    }
    println!("  - frontend/node_modules/");
    println!();
    println!("Se conservará: el código Go, el frontend y el Taskfile.");
    println!();

    let confirm = inquire::Confirm::new("¿Estás seguro?")
        .with_default(false)
        .prompt()?;
    if !confirm {
        println!("[INFO] Operación cancelada.");
        ui::pause();
        return Ok(());
    }

    for (label, path) in clean_targets(ctx) {
        if !path.exists() {
            println!("  [--] No hay {}", label);
            continue;
        }
        match cmd::remove_dir(&path) {
            Ok(()) => println!("  [OK] {} eliminado", label),
            Err(e) => println!("  [WARN] No se pudo borrar {}: {}", label, e),
        }
    }

    let nm = ctx.script_dir.join("frontend").join("node_modules");
    if nm.exists() {
        match cmd::remove_dir(&nm) {
            Ok(()) => println!("  [OK] frontend/node_modules eliminado"),
            Err(e) => println!("  [WARN] No se pudo borrar frontend/node_modules: {}", e),
        }
    }

    println!();
    println!("[OK] Limpieza completada.");
    ui::pause();
    Ok(())
}
