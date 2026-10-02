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

/// Todo este módulo manipula el registro de Windows (`ExtendedSubCommandsKey`
/// para el menú cascada), que no existe en Linux. La tarjeta está oculta en el
/// menú, pero las tasks siguen alcanzables por CLI: se informa en vez de
/// invocar PowerShell y reportar un fallo genérico.
fn unsupported_on_this_platform(action: &str) -> Result<()> {
    println!();
    println!("[INFO] El menú contextual sólo está disponible en Windows.");
    println!("       {action} requiere el registro de Windows (`HKCU\\Software\\Classes`).");
    println!();
    println!("En Linux el equivalente sería una entrada .desktop con `Actions=`,");
    println!("que no está implementada. Las tarjetas 'Code Runner' y 'Vibe Coding'");
    println!("del menú contextual no están soportadas en esta plataforma.");
    println!();
    Ok(())
}

pub fn install() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return unsupported_on_this_platform("Instalar el menú contextual");
    }

    let exe = exe_path();

    if command_is_current("zz_CodeRunner", &exe) && label_is("zz_CodeRunner", "Code Runner") {
        println!("[SKIP] Menú contextual Code Runner ya está instalado.");
        println!("  Ruta: {exe}");
        return Ok(());
    }

    let icon = icon_path();

    println!("[INFO] Icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Name '(default)' -Value 'Code Runner'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner\\command' -Name '(default)' -Value '\"{exe}\" -p \"%1\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Name '(default)' -Value 'Code Runner'; \
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
    println!("  Haz clic derecho en una carpeta → 'Code Runner'");
    println!();
    println!("[INFO] Si el icono no aparece, intenta:");
    println!("  1. Cerrar sesion y volver a iniciar");
    println!("  2. O ejecutar en PowerShell: ie4uinit.exe -show");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return unsupported_on_this_platform("Desinstalar el menú contextual");
    }

    let ps = "\
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue";

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", ps]);

    println!("[OK] Menú contextual Code Runner eliminado.");
    Ok(())
}

// ── Vibe Coding (menú cascada: OpenCode + Kilo Code + Cline + Antigravity) ──
// Usa ExtendedSubCommandsKey (método oficial de Windows para menús cascada)

const VIBE_KEY: &str = "zz_VibeCoding";
const CTX_PATH: &str = "Directory\\ContextMenus\\VibeCoding";

/// Herramienta CLIexpuesta en el menú cascada "Vibe Coding".
/// El ORDEN de este array define el orden en el submenú (Windows no tiene
/// mechanism de orden explícito: ordena alfabéticamente por nombre de clave).
struct VibeTool {
    /// Nombre de la clave de registro (determina el orden alfabético).
    key: &'static str,
    /// Texto visible en el menú.
    title: &'static str,
    /// Comando a ejecutar.
    cli: &'static str,
}

/// Orden solicitado: OpenCode → Kilo Code → Cline → Antigravity.
/// Los prefijos numéricos fuerzan ese orden porque Windows ordena las
/// sub-claves de `shell` alfabéticamente (A-Z, por tanto 01 < 02 < 03 < 04).
const VIBE_TOOLS: &[VibeTool] = &[
    VibeTool { key: "01OpenCode", title: "OpenCode", cli: "opencode --auto" },
    VibeTool { key: "02KiloCode", title: "Kilo Code", cli: "kilo" },
    VibeTool { key: "03Cline", title: "Cline", cli: "cline -i" },
    VibeTool { key: "04Antigravity", title: "Antigravity", cli: "agy" },
];

/// Resuelve el ejecutable real de una herramienta de Vibe Coding.
/// Primero busca en PATH; para `agy` (binario nativo, no npm) hace fallback
/// a la ruta de instalación oficial de Antigravity en Windows
/// (`%LOCALAPPDATA%\agy\bin\agy.exe`), que NO se agrega al PATH por defecto.
fn resolve_tool_command(tool: &VibeTool) -> Option<(String, String)> {
    if command_exists(tool.cli.split_whitespace().next().unwrap_or(tool.cli)) {
        return Some((tool.cli.to_string(), String::new()));
    }

    // Fallback: ruta conocida de instalación de Antigravity CLI.
    if tool.title == "Antigravity" {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let candidate = std::path::Path::new(&local).join("agy").join("bin").join("agy.exe");
            if candidate.exists() {
                if let Some(p) = candidate.to_str() {
                    return Some((p.to_string(), p.to_string()));
                }
            }
        }
    }

    None
}

/// Instala el menú cascada "Vibe Coding" con las sub-entradas que estén disponibles.
pub fn install_vibe_coding() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return unsupported_on_this_platform("Instalar el menú 'Vibe Coding'");
    }

    let icon = opencode_icon();

    // Limpiar entradas viejas individuales (OpenCode/Cline/Kilo sueltos)
    remove_vibe_old_keys();

    // Recrear el árbol de sub-entradas desde cero. Necesario para eliminar
    // claves huérfanas de versiones previas (p.ej. `OpenCode` sin el prefijo
    // numérico, que se acumulaban y alteraban el orden del submenú).
    remove_vibe_ctx_tree();

    // Resolver cada herramienta: comando real a ejecutar + nota de ruta
    let resolved: Vec<(&VibeTool, String, Option<String>)> = VIBE_TOOLS
        .iter()
        .filter_map(|t| {
            resolve_tool_command(t).map(|(cmd, note)| (t, cmd, if note.is_empty() { None } else { Some(note) }))
        })
        .collect();

    if resolved.is_empty() {
        println!("[WARN] No se encontró opencode, kilo, cline ni agy.");
        println!("[WARN] Se instalará el menú Vibe Coding sin sub-entradas.");
        println!("[WARN] Instala las CLIs y vuelve a ejecutar el instalador.");
    }

    println!("[INFO] Detectando herramientas Vibe Coding...");
    for tool in VIBE_TOOLS {
        match resolved.iter().find(|(t, _, _)| t.key == tool.key) {
            Some((_, _, Some(path))) => println!("  ✔ {:<14} (PATH no lo expone, usando: {})", tool.title, path),
            Some(_) => println!("  ✔ {}", tool.title),
            None => println!("  ✘ {} no encontrado", tool.title),
        }
    }

    let mut ps = String::new();

    // ── Padre en Directory\shell (usa MUIVerb + ExtendedSubCommandsKey) ──
    ps.push_str(&format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Name 'MUIVerb' -Value 'Vibe Coding' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Name 'Icon' -Type String -Value '{icon}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Name 'ExtendedSubCommandsKey' -Value '{CTX_PATH}' -Force; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Name 'MUIVerb' -Value 'Vibe Coding' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Name 'Icon' -Type String -Value '{icon}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Name 'ExtendedSubCommandsKey' -Value '{CTX_PATH}' -Force; "
    ));

    // ── Sub-entradas en ContextMenus (ruta compartida para Directory y Background) ──
    for (tool, cmdline, _) in &resolved {
        ps.push_str(&format!(
            "New-Item -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}\\shell\\{key}' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}\\shell\\{key}' -Name 'MUIVerb' -Value '{title}' -Force; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}\\shell\\{key}' -Name 'Icon' -Type String -Value '{icon}' -Force; \
             New-Item -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}\\shell\\{key}\\command' -Force | Out-Null; \
             Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}\\shell\\{key}\\command' -Name '(default)' -Value 'cmd.exe /c start \"{title}\" /D \"%V\" cmd.exe /c \"{cmdline}\"'; ",
            key = tool.key,
            title = tool.title,
            icon = icon,
            cmdline = cmdline
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
    if resolved.is_empty() {
        println!("  [WARN] Sin sub-entradas: ninguna CLI encontrada.");
    }
    for (tool, _, note) in &resolved {
        if let Some(path) = note {
            println!("  ✔ {:<14} -> {}", tool.title, path);
        }
    }
    println!("  Clic derecho en una carpeta → 'Vibe Coding'");
    println!();
    println!("[INFO] Si el menú no aparece, reinicia el Explorador de Windows:");
    println!("  taskkill /f /im explorer.exe & start explorer.exe");
    Ok(())
}

pub fn uninstall_vibe_coding() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return unsupported_on_this_platform("Desinstalar el menú 'Vibe Coding'");
    }

    let ps = format!(
        "Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{VIBE_KEY}' -Recurse -Force -ErrorAction SilentlyContinue; \
         Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{VIBE_KEY}' -Recurse -Force -ErrorAction SilentlyContinue; \
         Remove-Item -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}' -Recurse -Force -ErrorAction SilentlyContinue"
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

pub fn install_antigravity() -> Result<()> {
    install_vibe_coding()
}

pub fn uninstall_antigravity() -> Result<()> {
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
    // Vibe Coding usa ExtendedSubCommandsKey en vez de command
    let ps = if name == VIBE_KEY {
        format!(
            "Test-Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{name}'"
        )
    } else {
        format!(
            "Test-Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{name}\\command'"
        )
    };
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

/// Verifica que el texto visible (`(default)` o `MUIVerb`) de la entrada
/// coincida con el esperado. Permite forzar el reinstall cuando solo cambia
/// el nombre del menú, aunque el comando ya sea el correcto.
fn label_is(name: &str, expected: &str) -> bool {
    let ps = format!(
        "$v = (Get-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{name}' -ErrorAction SilentlyContinue); \
         if ($null -eq $v) {{ 'false' }} else {{ if ($v.MUIVerb) {{ $v.MUIVerb }} else {{ $v.'(default)' }} }}"
    );
    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == expected,
        Err(_) => false,
    }
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

/// Elimina el árbol completo de sub-entradas de Vibe Coding para reconstruirlo
/// limpio en cada instalación (evita claves huérfanas que alteran el orden).
fn remove_vibe_ctx_tree() {
    let ps = format!(
        "Remove-Item -Path 'HKCU:\\Software\\Classes\\{CTX_PATH}' -Recurse -Force -ErrorAction SilentlyContinue"
    );
    let _ = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
