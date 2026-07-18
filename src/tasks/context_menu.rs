use std::io::{self, Write};
use anyhow::Result;
use crate::utils::cmd;

fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn icon_path() -> String {
    exe_dir()
        .map(|d| d.join("icons8-terminal-64.ico"))
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| {
            let home = std::env::var("USERPROFILE").unwrap_or_default();
            format!("{}\\.cargo\\bin\\icons8-terminal-64.ico", home)
        })
}

pub fn install() -> Result<()> {
    let exe = exe_dir()
        .and_then(|d| d.join("code-runner.exe").to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "code-runner.exe".to_string());
    let icon = icon_path();

    println!("[INFO] Icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Name '(default)' -Value 'Abrir con Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner' -Name 'Icon' -PropertyType String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%1\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Name '(default)' -Value 'Abrir con Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner' -Name 'Icon' -PropertyType String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%W\"'"
    );

    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al instalar menú contextual:\n{}", stderr);
    }

    println!("[OK] Menú contextual instalado.");
    println!("  Ruta: {exe}");
    println!("  Icono: {icon}");
    println!("  Haz clic derecho en una carpeta → 'Abrir con Code Runner'");
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

    println!("[OK] Menú contextual eliminado.");
    Ok(())
}

pub fn toggle() -> Result<()> {
    loop {
        cmd::clear_screen();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║     MENÚ CONTEXTUAL DE WINDOWS                  ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();
        println!("  Al instalar, aparecerá 'Abrir con Code Runner'");
        println!("  al hacer clic derecho sobre cualquier carpeta.");
        println!();

        let installed = is_installed();

        let options = vec![
            if installed { "1. Desinstalar menú contextual".to_string() } else { "1. Instalar menú contextual".to_string() },
            "2. Volver".to_string(),
        ];

        let choice = inquire::Select::new("Selecciona una opción:", options)
            .with_vim_mode(true)
            .prompt()?;

        match choice.as_str() {
            s if s.starts_with("1.") => {
                if installed {
                    uninstall()?;
                } else {
                    install()?;
                }
                pause();
            }
            _ => break,
        }
    }
    Ok(())
}

fn is_installed() -> bool {
    let ps = "Test-Path 'HKCU:\\Software\\Classes\\Directory\\shell\\CodeRunner\\command' -PathType Leaf";
    let output = cmd::run("powershell", &["-NoProfile", "-Command", ps]);
    output.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True").unwrap_or(false)
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
