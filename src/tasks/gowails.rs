//! Soporte de **Go + Wails v3**.
//!
//! Un proyecto Wails v3 se reconoce por `go.mod` con `wailsapp/wails` y un
//! `Taskfile.yml` en la raíz (la v2 usaba `wails.json` y otro CLI, así que no
//! se mezcla: ver `context::is_wails_v3`).
//!
//! ## Por qué el build de Android **no** usa el Taskfile del proyecto
//!
//! Las tareas `android:*` de la plantilla de Wails v3 usan scripts POSIX, que
//! no se ejecutan en Windows. Aunque la NDK del host sí exista
//! (`windows-x86_64`), el build del Taskfile aborta. Por eso Compass hace el pipeline de Android
//! por su cuenta, replicando los pasos del Taskfile con el *host tag* y el
//! compilador correctos para Windows:
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

use crate::utils::android as android_utils;
use crate::utils::{cmd, context::ProjectContext, platform, ui};

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

/// Lee `productName` y `version` de `build/config.yml` (Wails v3).
fn read_wails_app_meta(ctx: &ProjectContext) -> Option<(String, String)> {
    let conf_path = ctx.script_dir.join("build").join("config.yml");
    let content = std::fs::read_to_string(&conf_path).ok()?;
    let mut name = None;
    let mut version = None;
    let mut in_info = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "info:" {
            in_info = true;
            continue;
        }
        if in_info && !trimmed.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            in_info = false;
        }
        if !in_info {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            // Quita comentarios y comillas del valor YAML simple.
            let val = v.split('#').next().unwrap_or(v).trim();
            let val = val.trim_matches('"').trim_matches('\'').to_string();
            if key == "productName" && name.is_none() {
                name = Some(val);
            } else if key == "version" && version.is_none() {
                version = Some(val);
            }
        }
    }
    Some((name?, version?))
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
    let ps = format!("$null -ne (Get-Command -Name '{name}' -ErrorAction SilentlyContinue)");
    cmd::run_ps(&ps)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True")
        .unwrap_or(false)
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

/// Carpeta de la toolchain LLVM de la NDK para Windows.
fn ndk_toolchain(ndk: &Path) -> Option<PathBuf> {
    let prebuilt = ndk.join("toolchains").join("llvm").join("prebuilt");
    let bin = prebuilt.join("windows-x86_64").join("bin");
    bin.is_dir().then_some(bin)
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
    None
}

/// `npm run <script>` en el frontend.
///
/// Se llama a `npm.cmd`: en PowerShell `npm` resuelve primero al shim
/// `npm.ps1`, que la ExecutionPolicy bloquea.
fn npm_run(dir: &Path, script: &str) -> Result<()> {
    let ok = cmd::run_ps_status(&format!("npm.cmd run {script}"), dir, &[])?;
    if !ok {
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

    // El binario de dev (`wails3 dev`) no genera el `.syso` (icono/version de
    // Windows) por defecto, así que el `.exe` sale con el icono genérico de Go.
    // Se genera antes, en el directorio del proyecto, para que `go build` (que
    // hace `wails3 dev` internamente) lo embeba. Es best-effort: si falla (p.ej.
    // no hay icono), el dev sigue funcionando, sólo sin icono.
    let _ = cmd::run_in_dir(
        "wails3",
        &[
            "generate",
            "syso",
            "-arch",
            "amd64",
            "-icon",
            "build/windows/icon.ico",
            "-manifest",
            "build/windows/wails.exe.manifest",
            "-info",
            "build/windows/info.json",
            "-out",
            "wails_windows_amd64.syso",
        ],
        &ctx.script_dir,
    );

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║          WAILS v3 - DEV (ESCRITORIO)             ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("[INFO] Comando: wails3 task dev");
    println!("[INFO] El servidor de Vite y la app se lanzan en la ventana nueva.");
    println!();

    cmd::spawn_new_window_interactive("Wails Dev", &ctx.script_dir, "wails3 task dev")?;

    Ok(())
}

/// Compila el **instalador** de escritorio (`wails3 task package`) y lo revela.
///
/// Antes esta tarjeta emitía `wails3 task build`, que sólo dejaba el `.exe`
/// portable. Para un release lo que se distribuye es el instalador:
///
/// | Host | Tarea | Artefacto en `bin/` |
/// |------|-------|---------------------|
/// | Windows | `windows:package` → NSIS (o MSIX con `FORMAT=msix`) | `<app>-<arch>-installer.exe` / `<app>-<arch>.msix` |
///
/// El Taskfile de la raíz despacha a `windows:package`.
///
/// ⚠️ **El filtro por nombre no es cosmético**: `package` compila primero el
/// binario (es una dependencia de la tarea) y lo deja en la misma carpeta
/// `bin/`, así que revelar "el archivo más reciente" podía enseñar el portable
/// en vez del instalador. Con `-installer` / `.msix` sólo entra el entregable.
pub fn build_desktop(ctx: &ProjectContext) -> Result<()> {
    ensure_wails_cli()?;

    println!("\n[INFO] Compilando instalador de escritorio con Wails...\n");
    println!("[INFO] Windows usa NSIS (`makensis` en el PATH). Para MSIX:");
    println!("[INFO]   wails3 task package FORMAT=msix");
    println!("[INFO]   (requiere `wails3 tool msix-install-tools`)");
    println!();

    let bin = bin_dir(ctx);

    // El instalador NSIS es un `.exe` más, así que se distingue del binario
    // portable por el nombre; `.msix`/`.msi` se reconocen por extensión.
    let rename = read_wails_app_meta(ctx).map(|(name, ver)| format!("{} v{}", name, ver));

    let exts: &[&str] = &["exe", "msix", "msi"];
    let names: &[&str] = &["installer", "msix", "msi"];
    cmd::run_build_and_reveal_named(
        "wails3 task package",
        &ctx.script_dir,
        &bin,
        exts,
        names,
        rename.as_deref(),
    )?;

    Ok(())
}

/// Alias de [`build_desktop`].
///
/// Existe sólo por el nombre del CLI (`--task package`). Antes eran dos rutas
/// distintas (`build` y `package`), pero el build de escritorio ya produce el
/// instalador, así que mantener dos implementaciones sólo servía para que se
/// desincronizaran.
pub fn package_desktop(ctx: &ProjectContext) -> Result<()> {
    build_desktop(ctx)
}

// ───────────────────────────────────────────────────────────────────────────
// Android
// ───────────────────────────────────────────────────────────────────────────

/// Compila el APK (dev o release) con el pipeline nativo de Compass.
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
    let overlay_arg = overlay.display().to_string();
    if !cmd::run_program(
        "wails3",
        &[
            "android",
            "overlay:gen",
            "-out",
            &overlay_arg,
            "-config",
            "build/config.yml",
        ],
        root,
        &[],
    )? {
        anyhow::bail!("[ERROR] Fallo `wails3 android overlay:gen`.");
    }

    // ── 2. Bindings para el frontend ──
    println!("[2/5] Generando los bindings...");
    // Sólo los **tags**: el generador reparte esta cadena por espacios y se
    // la pasa a `go list`, así que cualquier flag con valor compuesto
    // (`-ldflags="-w -s"`, `-gcflags=all="-l"`) lo rompe con
    // "flag provided but not defined: -s". Y para generar los bindings sólo
    // importan los tags: son los que deciden qué ficheros de Go se analizan.
    let build_flags = if release {
        "-tags production,android"
    } else {
        "-tags android,debug"
    };
    // ⚠️ **Aquí NO se pone `GOOS=android`**. El generador de bindings carga los
    // paquetes de Go para analizarlos, y con `GOOS=android` + `CGO_ENABLED=0`
    // el cargador no puede resolver ni la biblioteca estándar ("could not
    // import embed") → genera **0 servicios** y el frontend deja de compilar
    // ("Could not resolve ../../../bindings/<paquete>/service").
    // El Taskfile de Wails sí define una tarea `generate:android:bindings` con
    // esas variables, pero **no** forma parte del pipeline de build: la que se
    // usa (`common:generate:bindings`) corre en el host.
    let out = cmd::run_program_capture(
        "wails3",
        &[
            "generate",
            "bindings",
            "-f",
            build_flags,
            "-clean=true",
            "-ts",
            "-i",
        ],
        root,
        &[],
    )?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    print!("{stdout}");
    eprint!("{stderr}");

    if !out.status.success() {
        anyhow::bail!("[ERROR] Falló la generación de bindings.");
    }
    if stderr.contains("could not import") {
        anyhow::bail!(
            "[ERROR] El generador de bindings no pudo cargar los paquetes de Go.\n  \
             Suele pasar si se genera con GOOS=android: los bindings deben generarse\n  \
             en el host (sin GOOS=android) y sólo los *flags* llevan los tags de Android."
        );
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
    let jni_dir = android
        .join("app")
        .join("src")
        .join("main")
        .join("jniLibs")
        .join(JNI_DIR);
    std::fs::create_dir_all(&jni_dir)
        .with_context(|| format!("No se pudo crear: {}", jni_dir.display()))?;
    let so_path = jni_dir.join("libwails.so");
    let so_arg = so_path.display().to_string();

    // Los flags van como argumentos sueltos: sin shell, `-ldflags=-w -s`
    // llega a Go como un único argumento, que es lo que espera.
    let mut go_args: Vec<&str> = vec!["build", "-buildmode=c-shared", "-overlay", &overlay_arg];
    if release {
        go_args.extend_from_slice(&[
            "-tags",
            "production,android",
            "-trimpath",
            "-buildvcs=false",
            "-ldflags=-w -s",
        ]);
    } else {
        go_args.extend_from_slice(&[
            "-tags",
            "android,debug",
            "-buildvcs=false",
            "-gcflags=all=-l",
        ]);
    }
    go_args.extend_from_slice(&["-o", &so_arg]);

    let envs = vec![
        ("GOOS".to_string(), "android".to_string()),
        ("GOARCH".to_string(), ARCH.to_string()),
        ("CGO_ENABLED".to_string(), "1".to_string()),
        ("CC".to_string(), cc.clone()),
        ("CXX".to_string(), cxx.clone()),
    ];
    if !cmd::run_program("go", &go_args, root, &envs)? {
        anyhow::bail!(
            "[ERROR] Fallo la compilacion de libwails.so.\n\
             Revisa que la NDK sea compatible y que GOOS/GOARCH/CGO_ENABLED sean correctos."
        );
    }

    // ── 5. Gradle ──
    println!("[5/5] Ensamblando la APK con Gradle...");
    ensure_sdk_properties(ctx, &android)?;

    // La firma release llega al Gradle por variables de entorno. Sin ellas,
    // Wails firma con la llave de dev (que Play Store rechaza).
    let signing = if release { signing_env(ctx) } else { None };
    if release && signing.is_none() {
        println!(
            "[WARN] No hay keystore de release configurado; la APK se firmará con la llave de dev."
        );
        println!("       Configúralo en la tarjeta Signing (o con --task sign).");
    }

    let gradle_task = if release {
        "assembleRelease"
    } else {
        "assembleDebug"
    };
    if !gradle(&android, gradle_task, signing.as_deref().unwrap_or(&[]))? {
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
        println!(
            "[WARN] No se pudo copiar la APK a {}: {}",
            dest.display(),
            e
        );
    }

    Ok(apk_file)
}

/// Variables de entorno de firma que espera el Gradle de Wails.
///
/// Su `build/android/app/build.gradle` lee:
///
/// ```groovy
/// def keystoreFile = System.getenv("ANDROID_KEYSTORE_FILE")
/// ```
///
/// y, si está vacío, firma el release con la llave de **dev**. Compass
/// guarda la configuración en `android-signing/keystore.properties` (la
/// gestiona la tarjeta *Signing*) y la exporta aquí, para que el usuario no
/// tenga que definir variables a mano en cada build.
fn signing_env(ctx: &ProjectContext) -> Option<Vec<(String, String)>> {
    let layout = crate::tasks::signer::android_layout(ctx)?;
    let props = crate::tasks::signer::Props::read(&layout.props_path())?;
    let store_file = props.store_file?;
    let store_path = crate::tasks::signer::resolve_keystore(&layout.signing_dir, &store_file);
    if !store_path.exists() {
        return None;
    }
    let store_pass = props.store_password.unwrap_or_default();
    let key_pass = props.key_password.unwrap_or_else(|| store_pass.clone());
    let key_alias = props.key_alias?;
    Some(vec![
        (
            "ANDROID_KEYSTORE_FILE".to_string(),
            store_path.display().to_string(),
        ),
        ("ANDROID_KEYSTORE_PASSWORD".to_string(), store_pass),
        ("ANDROID_KEY_ALIAS".to_string(), key_alias),
        ("ANDROID_KEY_PASSWORD".to_string(), key_pass),
    ])
}

/// Ejecuta una tarea de Gradle en `build/android`.
///
/// Gradle se invoca con `.\`, que PowerShell exige para ejecutar un script del
/// directorio actual; el shell respeta el `dir` del proceso.
fn gradle(android: &Path, task: &str, envs: &[(String, String)]) -> Result<bool> {
    platform::find_gradlew(android).ok_or_else(|| {
        anyhow::anyhow!(
            "[ERROR] No se encontro el wrapper de Gradle en: {}\n\
             Regenera los assets de build con: wails3 update build-assets",
            android.display()
        )
    })?;
    cmd::run_ps_status(&format!(".\\gradlew.bat {task}"), android, envs)
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

/// APK **release** + instalación automática si hay un móvil conectado.
pub fn build_apk(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Release) con Wails...\n");
    let result = (|| -> Result<PathBuf> {
        let apk = build_android_apk(ctx, true)?;
        install_apk(ctx, &apk)?;
        Ok(apk)
    })();
    match result {
        Ok(_) => {
            ui::pause_for(10);
            Ok(())
        }
        Err(e) => {
            println!();
            println!("{}", e);
            println!();
            ui::pause();
            Err(e)
        }
    }
}

/// APK **dev** + instalación y arranque en el teléfono.
pub fn install_debug(ctx: &ProjectContext) -> Result<()> {
    println!("\n[INFO] Compilando Android APK (Dev) con Wails...\n");
    let result = (|| -> Result<PathBuf> {
        let apk = build_android_apk(ctx, false)?;
        install_apk(ctx, &apk)?;
        Ok(apk)
    })();
    match result {
        Ok(_) => {
            ui::pause_for(10);
            Ok(())
        }
        Err(e) => {
            println!();
            println!("{}", e);
            println!();
            ui::pause();
            Err(e)
        }
    }
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

    // Reutiliza el pipeline de la APK para tener `libwails.so` y el overlay.
    let result = (|| -> Result<PathBuf> {
        build_android_apk(ctx, true)?;

        if !gradle(
            &android,
            "bundleRelease",
            signing_env(ctx).as_deref().unwrap_or(&[]),
        )? {
            anyhow::bail!("[ERROR] Gradle fallo en `bundleRelease`.");
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
            println!(
                "[WARN] No se pudo copiar el AAB a {}: {}",
                dest.display(),
                e
            );
        }

        Ok(aab)
    })();
    match result {
        Ok(_) => {
            println!();
            println!("[OK] AAB generado correctamente.");
            println!(
                "[INFO] Un AAB no se instala directamente en el telefono; es para Play Store."
            );
            ui::pause_for(10);
            Ok(())
        }
        Err(e) => {
            println!();
            println!("{}", e);
            println!();
            ui::pause();
            Err(e)
        }
    }
}

/// Instala un APK ya compilado en el teléfono y abre la app.
///
/// **Se instala solo si hay un dispositivo Android conectado**: si no lo hay,
/// el paso se omite (la APK ya está compilada, que es lo que importa) y no se
/// pregunta nada.
fn install_apk(ctx: &ProjectContext, apk: &Path) -> Result<()> {
    let package = application_id(ctx);
    let activity = main_activity(ctx);
    android_utils::install_apk_if_device(ctx, apk, &package, &activity)?;
    Ok(())
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
    let pid = cmd::run(&adb, &["-s", &device.serial, "shell", "pidof", &package])
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let command = if pid.is_empty() {
        format!(
            "\"{adb}\" -s {} logcat -v time AndroidRuntime:E",
            device.serial
        )
    } else {
        format!(
            "\"{adb}\" -s {} logcat -v time --pid={}",
            device.serial, pid
        )
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
        (
            "frontend/bindings".into(),
            root.join("frontend").join("bindings"),
        ),
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

/// HMR en Android (`wails3 task android:dev`).
///
/// Reproduce el flujo descrito en el AGENTS del proyecto:
///
/// 1. Consola 1: `npm run dev` en `frontend/` (Vite, queda abierta).
/// 2. Consola 2: esta tarea, que compila el `.so` con el proxy al Vite de la
///    PC, publica el puerto con `adb reverse`, instala el APK de dev y
///    arranca la app. El `adb reverse tcp:PORT tcp:PORT` lo hace la propia
///    tarea, no hay que lanzarlo a mano.
///
/// Sin el Vite corriendo en `localhost:PORT`, la app abre pero no carga el
/// frontend, así que se avisa antes de lanzar nada.
pub fn dev_android(ctx: &ProjectContext) -> Result<()> {
    ensure_wails_cli()?;

    let port: u16 = std::env::var("WAILS_VITE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9245);

    // Comprueba que Vite esté escuchando en el puerto.
    let vite_ok = std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        std::time::Duration::from_secs(3),
    )
    .is_ok();

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       WAILS v3 - HMR EN ANDROID                  ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();
    println!("[INFO] Puerto de Vite esperado: http://localhost:{port}");
    println!();

    if !vite_ok {
        println!("[WARN] Vite no esta corriendo en el puerto {port}.");
        println!();
        println!("  Abre otra terminal, ve a frontend/ y ejecuta:");
        println!("    npm run dev");
        println!();
        println!("  Cuando Vite esté corriendo, vuelve a lanzar esta tarjeta.");
        println!("  (La tarea android:dev también lo comprueba, pero así lo ves antes.)");
        println!();
        ui::pause();
        return Ok(());
    }

    println!("[OK] Vite detectado en localhost:{port}.");
    println!();

    // ⚠️ **Parche del MIME type para HMR.** El WebView rechaza los módulos
    // de Vite (`@vite/client`, `src/main.ts`) porque el asset server de Wails
    // devuelve `application/octet-stream` para ellos. Sin el parche, la
    // pantalla queda en blanco. Se aplica una sola vez (es idempotente).
    patch_wails_bridge_mime(ctx);

    println!("[WARN] HMR en Android es experimental en Wails v3: la pantalla puede");
    println!("       quedar en blanco si el asset server no sirve algún módulo con");
    println!("       el MIME correcto. Si pasa, usa chrome://inspect para ver el");
    println!("       error exacto en la consola del WebView.");
    println!();

    println!("[INFO] Desinstalando la versión anterior (evita el choque de firma");
    println!("[INFO] debug ↔ dev que da INSTALL_FAILED_UPDATE_INCOMPATIBLE)...");
    let package = application_id(ctx);
    if let Some(adb) = ctx.adb_path.as_ref() {
        let _ = cmd::run(&adb.display().to_string(), &["uninstall", &package]);
    }
    println!();
    println!("[INFO] Lanzando: wails3 task android:dev");
    println!("[INFO] La tarea compila el .so, publica el puerto con `adb reverse`,");
    println!("[INFO] instala la APK de dev y arranca la app.");
    println!();

    cmd::spawn_new_window_interactive(
        "Wails Android HMR",
        &ctx.script_dir,
        "wails3 task android:dev",
    )?;

    Ok(())
}

/// Parchea `WailsBridge.java` para que sirva los archivos de dev (Vite HMR)
/// con el MIME type correcto.
///
/// El asset server de Wails v3 devuelve `application/octet-stream` para los
/// módulos de Vite (`@vite/client`, `src/main.ts`), y el WebView rechaza
/// cargarlos como módulos JS → pantalla en blanco. Este parche añade un
/// fallback por extensión.
///
/// El archivo está en `build/android/`, que `wails3 update build-assets`
/// regenera; por eso el parche se reaplica cada vez que se lanza la HMR.
fn patch_wails_bridge_mime(ctx: &ProjectContext) {
    let path = ctx
        .script_dir
        .join("build")
        .join("android")
        .join("app")
        .join("src")
        .join("main")
        .join("java")
        .join("com")
        .join("wails")
        .join("app")
        .join("WailsBridge.java");

    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    // Idempotencia: si ya está parcheado (con MimeTypeMap), no tocar.
    if content.contains("MimeTypeMap.getSingleton()") {
        return;
    }

    let old = r#"        try {
            String mimeType = nativeGetAssetMimeType(path);
            return mimeType != null ? mimeType : "application/octet-stream";
        } catch (Exception e) {
            return "application/octet-stream";
        }"#;

    let new = r#"        // El asset server de Wails devuelve "application/octet-stream" para los
        // módulos de Vite (@vite/client, src/main.ts) -> el WebView los rechaza.
        // Por eso se resuelve PRIMERO el MIME por extensión, antes de confiar
        // en nativeGetAssetMimeType. Se usa MimeTypeMap de Android para cubrir
        // todos los tipos (.ts, .svelte, .css, .js, .json, ...) sin listarlos
        // uno a uno.
        String lower = path.toLowerCase();
        int dot = lower.lastIndexOf('.');
        if (dot >= 0 && dot < lower.length() - 1) {
            String ext = lower.substring(dot + 1);
            String mime = android.webkit.MimeTypeMap.getSingleton()
                    .getMimeTypeFromExtension(ext);
            if (mime != null && !mime.isEmpty()) {
                return mime;
            }
        }
        if (lower.contains("@vite")) {
            return "application/javascript";
        }
        try {
            String mimeType = nativeGetAssetMimeType(path);
            if (mimeType != null && !mimeType.equals("application/octet-stream")) {
                return mimeType;
            }
        } catch (Exception e) {
            // fall through to the default below
        }
        return "application/octet-stream";"#;

    if let Some(pos) = content.find(old) {
        let patched = format!("{}{}{}", &content[..pos], new, &content[pos + old.len()..]);
        let _ = std::fs::write(&path, patched);
        println!("[INFO] Parche MIME aplicado a WailsBridge.java (HMR).");
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::context::ProjectType;

    fn ctx(root: &Path) -> ProjectContext {
        ProjectContext {
            script_dir: root.to_path_buf(),
            project_type: ProjectType::GoWails,
            android_home: None,
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        }
    }

    /// El keystore de Wails **no** puede vivir en `build/android/`: esa
    /// carpeta la regenera `wails3 update build-assets`.
    #[test]
    fn wails_keystore_lives_outside_build() {
        let root = Path::new("/tmp/proyecto");
        let layout = crate::tasks::signer::android_layout(&ctx(root)).unwrap();

        assert_eq!(
            layout.module_dir,
            root.join("build").join("android").join("app")
        );
        assert_eq!(layout.signing_dir, root.join("android-signing"));
        assert!(!layout.patch_gradle, "el Gradle de Wails ya lee el entorno");
        assert_eq!(
            layout.props_path(),
            root.join("android-signing").join("keystore.properties")
        );
    }

    /// Las variables que el Gradle de Wails espera vienen del `.properties`
    /// que gestiona la tarjeta *Signing*.
    #[test]
    fn signing_env_exports_the_wails_variables() {
        let dir = std::env::temp_dir().join("compass-wails-signing-test");
        let _ = std::fs::remove_dir_all(&dir);
        let signing = dir.join("android-signing");
        std::fs::create_dir_all(&signing).unwrap();
        std::fs::write(signing.join("release.keystore"), "x").unwrap();
        std::fs::write(
            signing.join("keystore.properties"),
            "storeFile=release.keystore\nstorePassword=storepw\nkeyAlias=upload\nkeyPassword=keypw\n",
        )
        .unwrap();

        let envs = signing_env(&ctx(&dir)).expect("debe leer las properties");
        let get = |k: &str| {
            envs.iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        assert!(get("ANDROID_KEYSTORE_FILE").ends_with("release.keystore"));
        assert_eq!(get("ANDROID_KEYSTORE_PASSWORD"), "storepw");
        assert_eq!(get("ANDROID_KEY_ALIAS"), "upload");
        assert_eq!(get("ANDROID_KEY_PASSWORD"), "keypw");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sin keystore no hay variables: el release se firmaría con la llave de
    /// debug, así que el build avisa.
    #[test]
    fn signing_env_is_empty_without_keystore() {
        let dir = std::env::temp_dir().join("compass-wails-signing-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(signing_env(&ctx(&dir)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
