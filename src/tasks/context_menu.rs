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

// ── Code Runner (entry principal) ──────────────────────────────────────

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

// ── Vibe Coding (menú cascada: OpenCode + Cline + Kilo Code) ───────────

const VIBE_KEY: &str = "zz_VibeCoding";

/// Instala el menú cascada "Vibe Coding" con las sub-entradas que estén disponibles.
pub fn install_vibe_coding() -> Result<()> {
    let icon = opencode_icon();

    // Limpiar entradas viejas individuales
    remove_vibe_old_keys();

    // Verificar si ya está instalado
    let oc_ok = is_installed_sub(VIBE_KEY, "OpenCode");
    let cl_ok = is_installed_sub(VIBE_KEY, "Cline");
    let kl_ok = is_installed_sub(VIBE_KEY, "Kilo");

    if oc_ok && cl_ok && kl_ok {
        println!("[SKIP] Menú 'Vibe Coding' ya está instalado con todas las herramientas.");
        return Ok(());
    }

    // Detectar qué herramientas están disponibles en PATH
    let has_opencode = command_exists("opencode");
    let has_cline = command_exists("cline");
    let has_kilo = command_exists("kilo");

    if !has_opencode && !has_cline && !has_kilo {
        println!("[WARN] No se encontró opencode, cline ni kilo en PATH.");
        println!("[WARN] Se instalará el menú vacío. Puedes re-instalarlo después.");
    }

    println!("[INFO] Detectando herramientas Vibe Coding...");
    if has_opencode { println!("  ✔ OpenCode encontrado"); } else { println!("  ✘ OpenCode no encontrado"); }
    if has_cline   { println!("  ✔ Cline encontrado"); }      else { println!("  ✘ Cline no encontrado"); }
    if has_kilo    { println!("  ✔ Kilo Code encontrado"); }   else { println!("  ✘ Kilo Code no encontrado"); }

    // Crear entrada padre
    let mut ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Name '(default)' -Value 'Vibe Coding'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Name 'Icon' -Type String -Value '{icon}'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Name '(default)' -Value 'Vibe Coding'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Name 'Icon' -Type String -Value '{icon}';"
    );

    // Sub-entrada: OpenCode
    if has_opencode && !oc_ok {
        ps.push_str(&format!(
            "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\OpenCode' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\OpenCode' -Name '(default)' -Value 'OpenCode'; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\OpenCode' -Name 'Icon' -Type String -Value '{icon}'; \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\OpenCode\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\OpenCode\\command' -Name '(default)' -Value 'cmd.exe /c start \"OpenCode\" /D \"%1\" cmd.exe /c \"opencode --auto\"'; \
             \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\OpenCode' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\OpenCode' -Name '(default)' -Value 'OpenCode'; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\OpenCode' -Name 'Icon' -Type String -Value '{icon}'; \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\OpenCode\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\OpenCode\\command' -Name '(default)' -Value 'cmd.exe /c start \"OpenCode\" /D \"%W\" cmd.exe /c \"opencode --auto\"';"
        ));
    }

    // Sub-entrada: Cline
    if has_cline && !cl_ok {
        ps.push_str(&format!(
            "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Cline' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Cline' -Name '(default)' -Value 'Cline'; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Cline' -Name 'Icon' -Type String -Value '{icon}'; \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Cline\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Cline\\command' -Name '(default)' -Value 'cmd.exe /c start \"Cline\" /D \"%1\" cmd.exe /c \"cline -i\"'; \
             \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Cline' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Cline' -Name '(default)' -Value 'Cline'; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Cline' -Name 'Icon' -Type String -Value '{icon}'; \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Cline\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Cline\\command' -Name '(default)' -Value 'cmd.exe /c start \"Cline\" /D \"%W\" cmd.exe /c \"cline -i\"';"
        ));
    }

    // Sub-entrada: Kilo Code
    if has_kilo && !kl_ok {
        ps.push_str(&format!(
            "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Kilo' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Kilo' -Name '(default)' -Value 'Kilo Code'; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Kilo' -Name 'Icon' -Type String -Value '{icon}'; \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Kilo\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}\\shell\\Kilo\\command' -Name '(default)' -Value 'cmd.exe /c start \"Kilo Code\" /D \"%1\" cmd.exe /c \"kilo\"'; \
             \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Kilo' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Kilo' -Name '(default)' -Value 'Kilo Code'; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Kilo' -Name 'Icon' -Type String -Value '{icon}'; \
             New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Kilo\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}\\shell\\Kilo\\command' -Name '(default)' -Value 'cmd.exe /c start \"Kilo Code\" /D \"%W\" cmd.exe /c \"kilo\"';"
        ));
    }

    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al instalar menú Vibe Coding:\n{}", stderr);
    }

    println!();
    println!("[OK] Menú contextual 'Vibe Coding' instalado.");
    println!("  Icono: {icon}");
    println!("  Haz clic derecho en una carpeta → 'Vibe Coding'");
    println!();
    println!("[INFO] Si el icono no aparece, intenta:");
    println!("  1. Cerrar sesion y volver a iniciar");
    println!("  2. O ejecutar en PowerShell: ie4uinit.exe -show");
    Ok(())
}

pub fn uninstall_vibe_coding() -> Result<()> {
    let ps = format!(
        "Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Recurse -Force -ErrorAction SilentlyContinue; \
         Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Recurse -Force -ErrorAction SilentlyContinue"
    );

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);

    println!("[OK] Menú contextual 'Vibe Coding' eliminado.");
    Ok(())
}

pub fn install_opencode() -> Result<()> {
    install_vibe_coding()
}

pub fn uninstall_opencode() -> Result<()> {
    uninstall_vibe_coding()
}

pub fn install_cline() -> Result<()> {
    install_vibe_coding()
}

pub fn uninstall_cline() -> Result<()> {
    uninstall_vibe_coding()
}

pub fn install_kilocode() -> Result<()> {
    install_vibe_coding()
}

pub fn uninstall_kilocode() -> Result<()> {
    uninstall_vibe_coding()
}

// ── Toggle interactivo ─────────────────────────────────────────────────

pub fn toggle() -> Result<()> {
    loop {
        cmd::clear_screen();
        println!("╔══════════════════════════════════════════════════╗");
        println!("║     MENÚ CONTEXTUAL DE WINDOWS                  ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();

        let cr_installed = is_installed("zz_CodeRunner");
        let vc_installed = is_installed(VIBE_KEY);

        let cr_label = if cr_installed { "✔ Code Runner" } else { "  Code Runner" };
        let vc_label = if vc_installed { "✔ Vibe Coding" } else { "  Vibe Coding" };

        let options = vec![
            format!("1. {cr_label}  [{}]", if cr_installed { "Desinstalar" } else { "Instalar" }),
            format!("2. {vc_label}  [{}]", if vc_installed { "Desinstalar" } else { "Instalar" }),
            "3. Instalar todos".to_string(),
            "4. Desinstalar todos".to_string(),
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
                if vc_installed { uninstall_vibe_coding()?; } else { install_vibe_coding()?; }
                pause();
            }
            s if s.starts_with("3.") => {
                install()?;
                install_vibe_coding()?;
                pause();
            }
            s if s.starts_with("4.") => {
                uninstall()?;
                uninstall_vibe_coding()?;
                pause();
            }
            _ => break,
        }
    }
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────

fn is_installed(name: &str) -> bool {
    let ps = format!(
        "Test-Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{name}\\command'"
    );
    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
    output.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True").unwrap_or(false)
}

fn is_installed_sub(parent: &str, sub: &str) -> bool {
    let ps = format!(
        "Test-Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{parent}\\shell\\{sub}\\command'"
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

fn command_exists(name: &str) -> bool {
    let ps = format!(
        "$null -ne (Get-Command -Name '{name}' -ErrorAction SilentlyContinue)"
    );
    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
    output.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True").unwrap_or(false)
}

/// Elimina entradas viejas individuales de OpenCode, Cline y Kilo Code.
fn remove_vibe_old_keys() {
    let old_keys = ["zz_OpenCode", "zz_pCline", "zz_pKilo"];
    let mut ps = String::new();
    for key in &old_keys {
        ps.push_str(&format!(
            "Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{key}' -Recurse -Force -ErrorAction SilentlyContinue; \
             Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{key}' -Recurse -Force -ErrorAction SilentlyContinue; "
        ));
    }
    if !ps.is_empty() {
        let _ = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
    }
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
