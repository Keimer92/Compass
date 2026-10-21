use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use inquire::Select;

use crate::utils::{cmd, context::ProjectContext};

/// Dispositivo Android disponible para instalar/ejecutar.
///
/// Sólo dispositivos físicos: el emulador se eliminó del flujo. Cualquier
/// serie que empiece por `emulator-` se filtra, de modo que un emulador que
/// estuviera corriendo no se confunde con un teléfono.
#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
}

impl Device {
    fn label(&self) -> String {
        format!("Físico: {}", self.serial)
    }
}

/// Comprueba que un dispositivo físico haya terminado de arrancar.
///
/// `sys.boot_completed` pasa a `1` cuando el sistema termina de iniciar. Un
/// teléfono recién conectado puede aparecer en `adb devices` como `device` y
/// aun así no aceptar un `adb install` hasta que acaba de arrancar, así que
/// esperar aquí evita un fallo confuso más abajo.
pub fn wait_for_device_boot(adb_path: &Path, serial: &str, timeout_secs: u64) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            anyhow::bail!(
                "[ERROR] Timeout esperando a que el dispositivo {} termine de arrancar ({}s).",
                serial,
                timeout_secs
            );
        }

        if let Ok(o) = cmd::run(
            adb_path.to_str().context("Ruta de adb inválida")?,
            &["-s", serial, "shell", "getprop", "sys.boot_completed"],
        ) {
            if String::from_utf8_lossy(&o.stdout).trim() == "1" {
                println!("[OK] Dispositivo listo: {}", serial);
                return Ok(());
            }
        }

        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Elige el dispositivo físico sobre el que trabajar.
///
/// Con un solo teléfono conectado lo auto-selecciona sin mostrar menú; con
/// varios, pregunta. Si no hay ninguno, da un mensaje accionable en vez de
/// caer a un emulador.
pub fn select_device(ctx: &ProjectContext) -> Result<Device> {
    let Some(adb) = ctx.adb_path.as_ref() else {
        anyhow::bail!(
            "[ERROR] No se encontró adb.\n\
             Instala platform-tools y define ANDROID_HOME:\n  \
             sdkmanager \"platform-tools\"\n\
             Y revisa que el teléfono tenga depuración USB activada."
        );
    };

    let phys = list_physical_devices(adb)?;

    match phys.len() {
        0 => anyhow::bail!(
            "[ERROR] No se detectó ningún dispositivo Android.\n\
             \n\
             Para conectarlo:\n\
             \x20 1. Activa 'Depuración USB' en Opciones de desarrollador del teléfono\n\
             \x20 2. Conéctalo por cable (o usa depuración por Wi-Fi)\n\
             \x20 3. Acepta la huella RSA que aparece en pantalla\n\
             \x20 4. Desbloquea la pantalla: el dispositivo sólo aparece cuando\n\
             \x20    está desbloqueado y en modo depuración\n\
             \n\
             Verifica con: adb devices -l"
        ),
        1 => {
            let device = phys.into_iter().next().expect("longitud 1");
            println!("[OK] Dispositivo físico detectado: {}", device.serial);
            wait_for_device_boot(adb, &device.serial, 60)?;
            Ok(device)
        }
        _ => {
            let labels: Vec<String> = phys.iter().map(|d| d.label()).collect();
            let selection = Select::new("Varios dispositivos. Selecciona uno:", labels).prompt()?;
            let idx = phys
                .iter()
                .position(|d| d.label() == selection)
                .context("Selección inválida")?;
            let device = phys.into_iter().nth(idx).expect("índice válido");
            wait_for_device_boot(adb, &device.serial, 60)?;
            Ok(device)
        }
    }
}

/// Instala un APK **ya compilado** en el dispositivo físico y abre la app.
///
/// Es el equivalente de `adb install -r` + `am start`, para los casos en los
/// que no hay una tarea de Gradle que instale (Kotlin y Go+Wails compilan el
/// APK por su cuenta).
///
/// `package` es el `applicationId` y `activity` la clase **completa** de la
/// actividad principal (`com.wails.app.MainActivity`), no el alias corto.
///
/// ⚠️ El **choque de firmas** es el detalle importante: si el teléfono ya tiene
/// la app instalada con otra clave (por ejemplo la de *debug* frente a la de
/// *release*), `adb install -r` falla con `INSTALL_FAILED_UPDATE_INCOMPATIBLE`.
/// En ese caso se ofrece desinstalar la versión anterior y reinstalar.
pub fn install_apk(
    ctx: &ProjectContext,
    apk: &Path,
    package: &str,
    activity: &str,
) -> Result<()> {
    let adb = ctx
        .adb_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "adb".to_string());

    let device = select_device(ctx)?;
    let apk_str = apk.display().to_string();

    let output = cmd::run(&adb, &["-s", &device.serial, "install", "-r", &apk_str])?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    let mut installed = combined.contains("Success");

    if !installed
        && (combined.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE")
            || combined.contains("signatures do not match")
            || combined.contains("INSTALL_FAILED_VERSION_DOWNGRADE"))
    {
        println!("[WARN] La app ya instalada esta firmada con otra clave (debug).");
        if !package.is_empty() {
            let uninstall = inquire::Confirm::new(
                "¿Desinstalar la version anterior e instalar la release?",
            )
            .with_default(true)
            .prompt()?;
            if uninstall {
                let _ = cmd::run(&adb, &["-s", &device.serial, "uninstall", package]);
                let output = cmd::run(&adb, &["-s", &device.serial, "install", "-r", &apk_str])?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = format!("{stdout}\n{stderr}");
                installed = combined.contains("Success");
                if !installed {
                    println!("[ERROR] La instalacion fallo:");
                    println!("{}", combined.trim());
                }
            }
        }
    } else if !installed {
        println!("[ERROR] La instalacion fallo:");
        println!("{}", combined.trim());
    }

    if installed {
        println!("[OK] APK instalado en {}", device.serial);

        if !package.is_empty() && !activity.is_empty() {
            let component = format!("{package}/{activity}");
            let _ = cmd::run(
                &adb,
                &["-s", &device.serial, "shell", "am", "start", "-n", &component],
            );
            println!("[OK] Abriendo aplicacion...");
        }
    }

    Ok(())
}

/// Dispositivos físicos conectados **sin preguntar nada**.
///
/// Se usa para decidir si tiene sentido instalar: si no hay ninguno, la
/// instalación se omite (la APK ya está compilada, eso es lo que importa).
pub fn connected_devices(ctx: &ProjectContext) -> Vec<Device> {
    let Some(adb) = ctx.adb_path.as_ref() else {
        return Vec::new();
    };
    list_physical_devices(adb).unwrap_or_default()
}

/// Instala el APK **si hay un dispositivo físico conectado**.
///
/// Devuelve si se llegó a instalar. Sin dispositivo no es un error: el build
/// ya terminó y la APK está lista; sólo se omite el paso y no se pregunta
/// nada.
///
/// Con varios dispositivos conectados, `install_apk` pregunta cuál (a través
/// de `select_device`).
///
/// ⚠️ **Nunca aborta el build**: si la instalación falla, se avisa y se sigue.
/// La APK ya está compilada, que es lo que importa; y como la ventana del
/// menú se cierra al terminar el proceso, un error aquí se perdería de vista.
pub fn install_apk_if_device(
    ctx: &ProjectContext,
    apk: &Path,
    package: &str,
    activity: &str,
) -> Result<bool> {
    let devices = connected_devices(ctx);
    if devices.is_empty() {
        println!();
        println!("[INFO] No hay ningun dispositivo Android conectado: no se instala.");
        println!("[OK] APK disponible en: {}", apk.display());
        return Ok(false);
    }

    println!("[INFO] Dispositivo Android detectado: {}", devices[0].serial);
    if let Err(e) = install_apk(ctx, apk, package, activity) {
        println!();
        println!("[WARN] No se pudo instalar en el dispositivo:");
        println!("       {}", e.to_string().replace('\n', "\n       "));
        println!();
    }
    println!("[OK] APK disponible en: {}", apk.display());
    Ok(true)
}

/// Lista los dispositivos físicos conectados vía `adb devices`.
fn list_physical_devices(adb_path: &Path) -> Result<Vec<Device>> {
    let adb = adb_path.to_str().context("Ruta de adb inválida")?;
    let Ok(output) = cmd::run(adb, &["devices", "-l"]) else {
        anyhow::bail!(
            "[ERROR] No se pudo ejecutar adb.\n\
             Verifica que platform-tools esté instalado y que el servidor\n\
             adb funcione: adb kill-server && adb start-server"
        );
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices: Vec<Device> = stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let t = line.trim();
            if t.is_empty() {
                return None;
            }
            // Estados válidos: `device` (listo), `offline`, `unauthorized`,
            // `recovery`, `sideload`. Sólo `device` sirve para instalar.
            let (serial, rest) = t.split_once(char::is_whitespace)?;
            if !rest.trim_start().starts_with("device") {
                return None;
            }
            // Las series `emulator-*` son de emuladores, no dispositivos reales.
            if serial.is_empty() || serial.starts_with("emulator-") {
                return None;
            }
            Some(Device { serial: serial.to_string() })
        })
        .collect();

    devices.sort_by(|a, b| a.serial.cmp(&b.serial));
    devices.dedup_by(|a, b| a.serial == b.serial);

    // Diagnóstico cuando hay líneas `adb devices` pero ninguna utilizable: lo
    // más común es `unauthorized`, que se arregla aceptando la huella RSA.
    if devices.is_empty() {
        for line in stdout.lines().skip(1) {
            let t = line.trim();
            if t.contains("unauthorized") {
                println!(
                    "[WARN] Hay un dispositivo pero no autorizado: {}",
                    t.split_whitespace().next().unwrap_or("?")
                );
                println!("       Desbloquea el teléfono y acepta la huella RSA que muestra.");
            } else if t.contains("offline") {
                println!(
                    "[WARN] Hay un dispositivo en estado 'offline': {}",
                    t.split_whitespace().next().unwrap_or("?")
                );
                println!("       Prueba: adb kill-server && adb start-server");
            }
        }
    }

    Ok(devices)
}
