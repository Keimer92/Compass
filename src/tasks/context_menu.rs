use std::io::{self, Write};
use anyhow::Result;
use crate::utils::cmd;

fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn resolve_icon(name: &str) -> String {
    let dir = exe_dir();
    let custom = dir.as_ref().map(|d| d.join(name));
    let fallback = dir.as_ref().map(|d| d.join("Default.ico"));

    custom
        .filter(|p| p.exists())
        .or_else(|| fallback)
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            let home = std::env::var("USERPROFILE").unwrap_or_default();
            format!("{}\\.cargo\\bin\\Default.ico", home)
        })
}

fn icon_path() -> String {
    resolve_icon("icon.ico")
}

fn opencode_icon() -> String {
    resolve_icon("opencode.ico")
}

fn exe_path() -> String {
    exe_dir()
        .and_then(|d| d.join("code-runner.exe").to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "code-runner.exe".to_string())
}

pub fn install() -> Result<()> {
    let exe = exe_path();
    let icon = icon_path();

    println!("[INFO] Icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Name '(default)' -Value 'Open with Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%1\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Name '(default)' -Value 'Open with Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%W\"'"
    );

    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al instalar menú contextual:\n{}", stderr);
    }

    println!("[OK] Menú contextual Code Runner instalado.");
    println!("  Ruta: {exe}");
    println!("  Icono: {icon}");
    println!("  Haz clic derecho en una carpeta → 'Open with Code Runner'");
    println!();
    println!("[INFO] Si el icono no aparece, intenta:");
    println!("  1. Cerrar sesion y volver a iniciar");
    println!("  2. O ejecutar en PowerShell: ie4uinit.exe -show");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let ps = "\
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue";

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", ps]);

    println!("[OK] Menú contextual Code Runner eliminado.");
    Ok(())
}

pub fn install_opencode() -> Result<()> {
    let icon = opencode_icon();

    println!("[INFO] OpenCode icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\OpenCode' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\OpenCode' -Name '(default)' -Value 'Open with OpenCode'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\OpenCode' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\OpenCode\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\OpenCode\\command' -Name '(default)' -Value 'cmd.exe /c start \"OpenCode\" /D \"%1\" cmd.exe /c \"opencode\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\OpenCode' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\OpenCode' -Name '(default)' -Value 'Open with OpenCode'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\OpenCode' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\OpenCode\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\OpenCode\\command' -Name '(default)' -Value 'cmd.exe /c start \"OpenCode\" /D \"%W\" cmd.exe /c \"opencode\"'"
    );

    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al instalar menú OpenCode:\n{}", stderr);
    }

    println!("[OK] Menú contextual 'Open with OpenCode' instalado.");
    println!("  Icono: {icon}");
    println!("  Haz clic derecho en una carpeta → 'Open with OpenCode'");
    println!();
    println!("[INFO] Si el icono no aparece, intenta:");
    println!("  1. Cerrar sesion y volver a iniciar");
    println!("  2. O ejecutar en PowerShell: ie4uinit.exe -show");
    Ok(())
}

pub fn uninstall_opencode() -> Result<()> {
    let ps = "\
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\OpenCode' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\OpenCode' -Recurse -Force -ErrorAction SilentlyContinue";

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", ps]);

    println!("[OK] Menú contextual 'Open with OpenCode' eliminado.");
    Ok(())
}

pub fn toggle() -> Result<()> {
    loop {
        cmd::clear_screen();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║     MENÚ CONTEXTUAL DE WINDOWS                  ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();

        let cr_installed = is_installed("CodeRunner");
        let oc_installed = is_installed("OpenCode");

        let cr_label = if cr_installed { "✔ Code Runner" } else { "  Code Runner" };
        let oc_label = if oc_installed { "✔ OpenCode" } else { "  OpenCode" };

        let options = vec![
            format!("1. {cr_label}  [{}]", if cr_installed { "Desinstalar" } else { "Instalar" }),
            format!("2. {oc_label}  [{}]", if oc_installed { "Desinstalar" } else { "Instalar" }),
            "3. Instalar ambos".to_string(),
            "4. Desinstalar ambos".to_string(),
            "5. Volver".to_string(),
        ];

        let choice = inquire::Select::new("Selecciona una opción:", options)
            .with_vim_mode(true)
            .prompt()?;

        match choice.as_str() {
            s if s.starts_with("1.") => {
                if cr_installed { uninstall()?; } else { install()?; }
                pause();
            }
            s if s.starts_with("2.") => {
                if oc_installed { uninstall_opencode()?; } else { install_opencode()?; }
                pause();
            }
            s if s.starts_with("3.") => {
                install()?;
                install_opencode()?;
                pause();
            }
            s if s.starts_with("4.") => {
                uninstall()?;
                uninstall_opencode()?;
                pause();
            }
            _ => break,
        }
    }
    Ok(())
}

fn is_installed(name: &str) -> bool {
    let ps = format!(
        "Test-Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{name}\\command'"
    );
    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
    output.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True").unwrap_or(false)
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
