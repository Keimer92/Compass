use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use inquire::Select;

use crate::utils::{cmd, context::ProjectContext};

#[derive(Debug, Clone)]
pub enum DeviceKind {
    Emulator { avd_name: String, running: bool },
    Physical,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub kind: DeviceKind,
}

impl Device {
    fn label(&self) -> String {
        match &self.kind {
            DeviceKind::Emulator { avd_name, running } => {
                let status = if *running { "🟢" } else { "⚫" };
                format!("{} Emulator: {} ({})", status, avd_name, self.serial)
            }
            DeviceKind::Physical => {
                format!("📱 Físico: {}", self.serial)
            }
        }
    }
}

pub fn list_avds(emulator_path: &Path) -> Result<Vec<String>> {
    let output = cmd::run(
        emulator_path.to_str().context("Ruta de emulador inválida")?,
        &["-list-avds"],
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let avds: Vec<String> = stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(avds)
}

pub fn running_emulators(adb_path: &Path) -> HashMap<String, String> {
    let output = match cmd::run(
        adb_path.to_str().expect("Ruta de adb inválida"),
        &["devices"],
    ) {
        Ok(o) => o,
        Err(_) => return HashMap::new(),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);

    let emu_serials: Vec<String> = stdout
        .lines()
        .skip(1)
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && t.contains("device")
                && !t.contains("offline")
                && t.starts_with("emulator-")
        })
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut map = HashMap::new();
    for serial in emu_serials {
        if let Ok(output) = cmd::run(
            adb_path.to_str().expect("Ruta de adb inválida"),
            &["-s", &serial, "emu", "avd", "name"],
        ) {
            let name = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() {
                map.insert(name, serial);
            }
        }
    }
    map
}

pub fn launch_emulator(emulator_path: &Path, avd_name: &str) -> Result<()> {
    println!("[INFO] Lanzando emulador '{}'...", avd_name);
    cmd::spawn_new_window(
        &format!("Android Emulator ({})", avd_name),
        emulator_path
            .parent()
            .context("No se pudo resolver directorio del emulador")?,
        &format!("{} -avd {}", emulator_path.display(), avd_name),
    )?;
    Ok(())
}

pub fn wait_for_emulator_boot(adb_path: &Path, serial: &str, timeout_secs: u64) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            anyhow::bail!(
                "[ERROR] Timeout esperando al dispositivo {} ({}s)",
                serial,
                timeout_secs
            );
        }

        let output = cmd::run(
            adb_path.to_str().context("Ruta de adb inválida")?,
            &["-s", serial, "shell", "getprop", "sys.boot_completed"],
        );

        if let Ok(o) = output {
            let val = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if val == "1" {
                println!("[OK] Dispositivo listo: {}", serial);
                return Ok(());
            }
        }

        std::thread::sleep(Duration::from_secs(3));
    }
}

pub fn wait_for_emulator_serial(adb_path: &Path, timeout_secs: u64) -> Result<String> {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            anyhow::bail!(
                "[ERROR] Timeout esperando que aparezca el emulador ({}s)",
                timeout_secs
            );
        }

        if let Ok(output) = cmd::run(
            adb_path.to_str().context("Ruta de adb inválida")?,
            &["devices"],
        ) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                let trimmed = line.trim();
                if !trimmed.is_empty()
                    && trimmed.contains("device")
                    && !trimmed.contains("offline")
                    && trimmed.starts_with("emulator-")
                {
                    let serial = trimmed.split_whitespace().next().unwrap_or("").to_string();
                    if !serial.is_empty() {
                        println!("[INFO] Emulador detectado: {}", serial);
                        return Ok(serial);
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_secs(3));
    }
}

pub fn select_device(ctx: &ProjectContext) -> Result<Device> {
    let mut options: Vec<Device> = Vec::new();

    // Physical devices first
    if let Some(adb) = &ctx.adb_path {
        if let Ok(output) = cmd::run(adb.to_str().unwrap(), &["devices", "-l"]) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || trimmed.contains("offline")
                    || !trimmed.contains("device")
                {
                    continue;
                }
                let serial = trimmed.split_whitespace().next().unwrap_or("").to_string();
                if serial.is_empty() || serial.starts_with("emulator-") {
                    continue;
                }
                options.push(Device {
                    serial,
                    kind: DeviceKind::Physical,
                });
            }
        }
    }

    // Emulators after
    let running_emus = ctx
        .adb_path
        .as_ref()
        .map(|adb| running_emulators(adb))
        .unwrap_or_default();

    if let Some(emu_path) = &ctx.emulator_path {
        if let Ok(avds) = list_avds(emu_path) {
            for avd in avds {
                let serial = running_emus.get(&avd).cloned().unwrap_or_default();
                let running = !serial.is_empty();
                options.push(Device {
                    serial,
                    kind: DeviceKind::Emulator { avd_name: avd, running },
                });
            }
        }
    }

    if options.is_empty() {
        anyhow::bail!(
            "No se encontraron dispositivos Android.\n\
             Conecta un dispositivo físico con depuración USB activada,\n\
             o crea un AVD con Android Studio (Tools > AVD Manager)."
        );
    }

    let labels: Vec<String> = options.iter().map(|d| d.label()).collect();

    let selection = Select::new("Selecciona un dispositivo:", labels).prompt()?;

    let idx = options
        .iter()
        .position(|d| d.label() == selection)
        .context("Selección inválida")?;

    let mut device = options.into_iter().nth(idx).unwrap();

    if let DeviceKind::Emulator {
        ref avd_name,
        running: false,
    } = &device.kind
    {
        if let Some(emu_path) = &ctx.emulator_path {
            launch_emulator(emu_path, avd_name)?;
        }
        if let Some(adb) = &ctx.adb_path {
            let serial = wait_for_emulator_serial(adb, 120)?;
            device.serial = serial;
            wait_for_emulator_boot(adb, &device.serial, 120)?;
        }
    }

    Ok(device)
}
