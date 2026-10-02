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
