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
fn cline_icon() -> String {
    // Cline y Kilo Code comparten el icono de OpenCode
    resolve_icon("opencode.ico")
}

fn kilocode_icon() -> String {
    // Cline y Kilo Code comparten el icono de OpenCode
    resolve_icon("opencode.ico")
}

/// Instala un menu contextual generico para una herramienta CLI de IA.
/// `key` debe empezar por 'p' minuscula para que Windows (orden alfabetico
/// insensible a mayusculas) la coloque justo despues de zz_OpenCode.
fn install_cli_tool(key: &str, title: &str, cli_cmd: &str, icon: &str) -> Result<()> {
    if command_is_current(key, cli_cmd) {
        println!("[SKIP] Menú contextual 'Open with {title}' ya está instalado.");
        return Ok(());
    }

    println!("[INFO] {title} icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\shell\\\\{key}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\shell\\\\{key}' -Name '(default)' -Value 'Open with {title}'; \
         Set-ItemProperty -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\shell\\\\{key}' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\shell\\\\{key}\\\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\shell\\\\{key}\\\\command' -Name '(default)' -Value 'cmd.exe /c start \"{title}\" /D \"%1\" cmd.exe /c \"{cli_cmd}\"'; \
         \
         New-Item -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\Background\\\\shell\\\\{key}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\Background\\\\shell\\\\{key}' -Name '(default)' -Value 'Open with {title}'; \
         Set-ItemProperty -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\Background\\\\shell\\\\{key}' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\Background\\\\shell\\\\{key}\\\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\Background\\\\shell\\\\{key}\\\\command' -Name '(default)' -Value 'cmd.exe /c start \"{title}\" /D \"%W\" cmd.exe /c \"{cli_cmd}\"'"
    );

    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al instalar menú {title}:\n{}", stderr);
    }

    println!("[OK] Menú contextual 'Open with {title}' instalado.");
    println!("  Icono: {icon}");
    println!("  Haz clic derecho en una carpeta → 'Open with {title}'");
    println!();
    println!("[INFO] Si el icono no aparece, intenta:");
    println!("  1. Cerrar sesion y volver a iniciar");
    println!("  2. O ejecutar en PowerShell: ie4uinit.exe -show");
    Ok(())
}

fn uninstall_key(key: &str, title: &str) -> Result<()> {
    let ps = format!(
        "Remove-Item -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\shell\\\\{key}' -Recurse -Force -ErrorAction SilentlyContinue; \
         Remove-Item -Path 'HKCU:\\\\Software\\\\Classes\\\\Directory\\\\Background\\\\shell\\\\{key}' -Recurse -Force -ErrorAction SilentlyContinue"
    );

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);

    println!("[OK] Menú contextual 'Open with {title}' eliminado.");
    Ok(())
}



fn exe_path() -> String {
    exe_dir()
        .and_then(|d| d.join("code-runner.exe").to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "code-runner.exe".to_string())
}

pub fn install() -> Result<()> {
    let exe = exe_path();

    if command_is_current("zz_CodeRunner", &exe) {
        println!("[SKIP] Menú contextual Code Runner ya está instalado.");
        println!("  Ruta: {exe}");
        return Ok(());
    }

    let icon = icon_path();

    println!("[INFO] Icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Name '(default)' -Value 'Open with Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%1\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Name '(default)' -Value 'Open with Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%W\"'"
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
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue";

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", ps]);

    println!("[OK] Menú contextual Code Runner eliminado.");
    Ok(())
}

pub fn install_opencode() -> Result<()> {
    if command_is_current("zz_OpenCode", "opencode --auto") {
        println!("[SKIP] Menú contextual 'Open with OpenCode' ya está instalado.");
        return Ok(());
    }

    let icon = opencode_icon();

    println!("[INFO] OpenCode icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_OpenCode' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_OpenCode' -Name '(default)' -Value 'Open with OpenCode'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_OpenCode' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_OpenCode\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_OpenCode\\command' -Name '(default)' -Value 'cmd.exe /c start \"OpenCode\" /D \"%1\" cmd.exe /c \"opencode --auto\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_OpenCode' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_OpenCode' -Name '(default)' -Value 'Open with OpenCode'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_OpenCode' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_OpenCode\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_OpenCode\\command' -Name '(default)' -Value 'cmd.exe /c start \"OpenCode\" /D \"%W\" cmd.exe /c \"opencode --auto\"'"
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
    uninstall_key("zz_OpenCode", "OpenCode")
}

pub fn install_cline() -> Result<()> {
    let icon = cline_icon();
    install_cli_tool("zz_pCline", "Cline", "cline -i", &icon)
}

pub fn uninstall_cline() -> Result<()> {
    uninstall_key("zz_pCline", "Cline")
}

pub fn install_kilocode() -> Result<()> {
    let icon = kilocode_icon();
    install_cli_tool("zz_pKilo", "Kilo Code", "kilo", &icon)
}

pub fn uninstall_kilocode() -> Result<()> {
    uninstall_key("zz_pKilo", "Kilo Code")
}

pub fn toggle() -> Result<()> {
    loop {
        cmd::clear_screen();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║     MENÚ CONTEXTUAL DE WINDOWS                  ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();

        let cr_installed = is_installed("zz_CodeRunner");
        let oc_installed = is_installed("zz_OpenCode");
        let cl_installed = is_installed("zz_pCline");
        let kl_installed = is_installed("zz_pKilo");

        let cr_label = if cr_installed { "✔ Code Runner" } else { "  Code Runner" };
        let oc_label = if oc_installed { "✔ OpenCode" } else { "  OpenCode" };
        let cl_label = if cl_installed { "✔ Cline" } else { "  Cline" };
        let kl_label = if kl_installed { "✔ Kilo Code" } else { "  Kilo Code" };

        let options = vec![
            format!("1. {cr_label}  [{}]", if cr_installed { "Desinstalar" } else { "Instalar" }),
            format!("2. {oc_label}  [{}]", if oc_installed { "Desinstalar" } else { "Instalar" }),
            format!("3. {cl_label}  [{}]", if cl_installed { "Desinstalar" } else { "Instalar" }),
            format!("4. {kl_label}  [{}]", if kl_installed { "Desinstalar" } else { "Instalar" }),
            "5. Instalar todos".to_string(),
            "6. Desinstalar todos".to_string(),
            "7. Volver".to_string(),
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
                if cl_installed { uninstall_cline()?; } else { install_cline()?; }
                pause();
            }
            s if s.starts_with("4.") => {
                if kl_installed { uninstall_kilocode()?; } else { install_kilocode()?; }
                pause();
            }
            s if s.starts_with("5.") => {
                install()?;
                install_opencode()?;
                install_cline()?;
                install_kilocode()?;
                pause();
            }
            s if s.starts_with("6.") => {
                uninstall()?;
                uninstall_opencode()?;
                uninstall_cline()?;
                uninstall_kilocode()?;
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

fn command_is_current(name: &str, needle: &str) -> bool {
    let ps = format!(
        "$v = (Get-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{name}\\command' -ErrorAction SilentlyContinue).'(default)'; \
         if ($null -eq $v) {{ 'false' }} else {{ $v.Contains('{needle}') }}"
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
