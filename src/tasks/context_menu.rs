use std::io::{self, Write};
use anyhow::Result;
use crate::utils::{cmd, ui};

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
            // Sin icono junto al binario, se busca en la carpeta de instalación de
            // cargo. En Linux el home es `HOME`, no `USERPROFILE` (que ahí no
            // existe y dejaba la ruta como "\.cargo\bin\Default.ico").
            let home = crate::utils::platform::home_dir().unwrap_or_default();
            home.join(".cargo").join("bin").join("Default.ico").display().to_string()
        })
}

fn icon_path() -> String {
    resolve_icon("icon.ico")
}

fn opencode_icon() -> String {
    resolve_icon("opencode.ico")
}

/// Ruta del ejecutable de compass.
///
/// En Linux no lleva extensión: el registro de Windows guarda
/// `"…\compass.exe" -p "%1"`, y apuntar a un `.exe` inexistente dejaba la
/// entrada del menú apuntando al vacío.
fn exe_path() -> String {
    let name = if crate::utils::platform::IS_WINDOWS {
        "compass.exe"
    } else {
        "compass"
    };
    exe_dir()
        .and_then(|d| d.join(name).to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| name.to_string())
}

// ── Compass (entry principal) ──────────────────────────────────────

/// Todo este módulo manipula el registro de Windows (`ExtendedSubCommandsKey`
/// para el menú cascada) en Windows, y la config `context_actions` de
/// cosmic-files en Linux. El enrutado por plataforma ocurre en las funciones
/// públicas; aquí abajo solo vive la implementación de Windows.
pub fn install() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return super::context_menu_cosmic::install();
    }

    let exe = exe_path();

    if command_is_current("zz_Compass", &exe) && label_is("zz_Compass", "Compass") {
        println!("[SKIP] Menú contextual Compass ya está instalado.");
        println!("  Ruta: {exe}");
        install_gitignore_action_windows()?;
        remove_old_code_runner_entry();
        return Ok(());
    }

    // Migración: el binario anterior se llamaba `code-runner` y dejaba en el
    // registro una entrada `zz_CodeRunner`. Sin borrarla, al instalar Compass
    // quedarían las dos: la vieja (apuntando al exe posiblemente inexistente) y
    // la nueva.
    remove_old_code_runner_entry();

    let icon = icon_path();

    println!("[INFO] Icon path: {icon}");
    println!("[INFO] Icon exists: {}", std::path::Path::new(&icon).exists());

    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_Compass' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_Compass' -Name '(default)' -Value 'Compass'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_Compass' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_Compass\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_Compass\\command' -Name '(default)' -Value '\"{exe}\" -p \"%1\"'; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_Compass' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_Compass' -Name '(default)' -Value 'Compass'; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_Compass' -Name 'Icon' -Type String -Value '{icon}'; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_Compass\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_Compass\\command' -Name '(default)' -Value '\"{exe}\" -p \"%W\"'"
    );

    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Error al instalar menú contextual:\n{}", stderr);
    }

    install_gitignore_action_windows()?;

    println!("[OK] Menú contextual Compass instalado.");
    println!("  Ruta: {exe}");
    println!("  Icono: {icon}");
    println!("  Haz clic derecho en una carpeta → 'Compass'");
    println!();
    println!("[INFO] Si el icono no aparece, intenta:");
    println!("  1. Cerrar sesion y volver a iniciar");
    println!("  2. O ejecutar en PowerShell: ie4uinit.exe -show");
    Ok(())
}

fn remove_old_code_runner_entry() {
    // El binario anterior se llamaba `code-runner` y dejaba esta entrada en el
    // registro. Se borra en cada instalación/desinstalación de Compass para que
    // no quede un menú "Code Runner" duplicado y, encima, apuntando a un `.exe`
    // que puede ya no existir.
    let ps = "\
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_CodeRunner' -Recurse -Force -ErrorAction SilentlyContinue";
    let _ = cmd::run("powershell", &["-NoProfile", "-Command", ps]);
}

const GITIGNORE_KEY: &str = "zz_VibeGitignore";
const GITIGNORE_LABEL: &str = "Compass: Add .gitignore";

/// Instala el ejecutable no interactivo como hermano inmediato de Vibe Coding.
/// Las claves se ordenan alfabéticamente, por eso `VibeGitignore` va después
/// de `VibeCoding`.
fn install_gitignore_action_windows() -> Result<()> {
    let exe = exe_path();
    let icon = icon_path();
    let folder_command = gitignore_context_command(&exe, "%1");
    let background_command = gitignore_context_command(&exe, "%V");
    let ps = format!(
        "New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{GITIGNORE_KEY}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{GITIGNORE_KEY}' -Name '(default)' -Value '{GITIGNORE_LABEL}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{GITIGNORE_KEY}' -Name 'Icon' -Type String -Value '{icon}' -Force; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{GITIGNORE_KEY}\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\{GITIGNORE_KEY}\\command' -Name '(default)' -Value '{folder_command}' -Force; \
         \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{GITIGNORE_KEY}' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{GITIGNORE_KEY}' -Name '(default)' -Value '{GITIGNORE_LABEL}' -Force; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{GITIGNORE_KEY}' -Name 'Icon' -Type String -Value '{icon}' -Force; \
         New-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{GITIGNORE_KEY}\\command' -Force | Out-Null; \
         Set-ItemProperty -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\{GITIGNORE_KEY}\\command' -Name '(default)' -Value '{background_command}' -Force"
    );
    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps])?;
    if !output.status.success() {
        anyhow::bail!(
            "Error al instalar la acción contextual .gitignore:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn gitignore_context_command(exe: &str, placeholder: &str) -> String {
    format!(
        "cmd.exe /c start \"\" /D \"{placeholder}\" cmd.exe /k \"\"{exe}\" --task context-gitignore --path \"{placeholder}\"\""
    )
}

pub fn uninstall() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return super::context_menu_cosmic::uninstall();
    }

    remove_old_code_runner_entry();

    let ps = "\
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_Compass' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_Compass' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\shell\\zz_VibeGitignore' -Recurse -Force -ErrorAction SilentlyContinue; \
Remove-Item -Path 'HKCU:\\Software\\Classes\\Directory\\Background\\shell\\zz_VibeGitignore' -Recurse -Force -ErrorAction SilentlyContinue";

    let _ = cmd::run("powershell", &["-NoProfile", "-Command", ps]);

    println!("[OK] Menú contextual Compass eliminado.");
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
    /// Comando oficial de instalación por terminal.
    ///
    /// Los tres primeros son paquetes npm globales; Antigravity es un
    /// instalador nativo. De ahí [`InstallVia`]: en Windows `npm` es `npm.cmd`
    /// y necesita `cmd`, mientras que `irm | iex` es sintaxis de PowerShell y
    /// en `cmd` no existe.
    install: &'static str,
    /// Rutas donde el instalador deja el ejecutable cuando **no** lo agrega al
    /// PATH. Los npm globales sí lo hacen, así que para ellos va vacía.
    ///
    /// Con prefijo `LOCALAPPDATA/` o `HOME/` se expanden; si no, es una ruta
    /// absoluta. Los instaladores nativos (Antigravity, Hermes) dejan el binario
    /// en su propia carpeta y no siempre tocan el PATH del usuario.
    fallback: &'static [&'static str],
    install_via: InstallVia,
}

/// Shell con el que hay que ejecutar [`VibeTool::install`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum InstallVia {
    /// `cmd /c …`. Lo que necesitan los `npm install -g`, porque en Windows
    /// `npm` es `npm.cmd` y PowerShell ejecuta el shim `npm.ps1`, que bloquea
    /// la ExecutionPolicy por defecto.
    Cmd,
    /// `powershell -Command …`. `irm | iex` no existe en `cmd`.
    PowerShell,
}

/// Orden solicitado: OpenCode → Kilo Code → Cline → Antigravity → Copilot CLI
/// → Kiro CLI → Hermes.
/// Los prefijos numéricos fuerzan ese orden porque Windows ordena las
/// sub-claves de `shell` alfabéticamente (01 < 02 < … < 07).
///
/// `cli` es lo que se ejecuta al elegir la entrada del menú; el primer token es
/// el binario que [`resolve_tool_command`] busca en el PATH. Para Kiro CLI ese
/// token es `kiro-cli`, que es como nombra el ejecutable el instalador.
///
/// Comandos de instalación verificados en la documentación oficial de cada
/// producto (opencode.ai/download, kilo.ai/docs, docs.cline.bot,
/// antigravity.google/docs, kiro.dev, hermes-agent.nousresearch.com y
/// docs.github.com/copilot/how-tos/copilot-cli).
const VIBE_TOOLS: &[VibeTool] = &[
    VibeTool {
        key: "01OpenCode",
        title: "OpenCode",
        cli: "opencode --auto",
        install: "npm install -g @opencode/cli",
        fallback: &[],
        install_via: InstallVia::Cmd,
    },
    VibeTool {
        key: "02KiloCode",
        title: "Kilo Code",
        // `--auto` activa el YOLO mode: aprueba todas las acciones automáticamente
        // sin pedir confirmación (equivalente al `--auto` de opencode).
        cli: "kilo --auto",
        install: "npm install -g @kilocode/cli",
        fallback: &[],
        install_via: InstallVia::Cmd,
    },
    VibeTool {
        key: "03Cline",
        title: "Cline",
        // `--auto-approve true` habilita la aprobación global de herramientas
        // sin prompts (modo autónomo documentado en docs.cline.bot/cline-cli/overview).
        cli: "cline --auto-approve true",
        install: "npm install -g cline",
        fallback: &[],
        install_via: InstallVia::Cmd,
    },
    VibeTool {
        key: "04Antigravity",
        title: "Antigravity",
        // `--dangerously-skip-permissions` es el equivalente de Antigravity al
        // `--dangerously-skip-permissions` de Claude Code: omite todos los prompts
        // de permisos de herramientas (documentado en google-antigravity/antigravity-cli).
        cli: "agy --dangerously-skip-permissions",
        install: "irm https://antigravity.google/cli/install.ps1 | iex",
        // El instalador de Google lo deja en su carpeta propia y no siempre lo
        // agrega al PATH.
        fallback: &[
            "LOCALAPPDATA/agy/bin/agy.exe",
            "HOME/agy/bin/agy",
            "HOME/.local/bin/agy",
            "/usr/local/bin/agy",
        ],
        install_via: InstallVia::PowerShell,
    },
    VibeTool {
        key: "05Copilot",
        title: "Copilot CLI",
        // `--yolo` habilita todos los permisos (equivale a `--allow-all`, o sea
        // `--allow-all-tools --allow-all-paths --allow-all-urls`), que es el
        // modo autónomo documentado en `copilot --help`.
        cli: "copilot --yolo",
        // A diferencia del resto, Copilot CLI se instala por winget: así lo pide
        // su documentación de Windows (`winget install GitHub.Copilot`). Los
        // `--accept-*` evitan los prompts interactivos de winget, que en esta
        // ejecución no tienen a quién preguntar.
        install: "winget install --id GitHub.Copilot --exact --accept-source-agreements --accept-package-agreements",
        // winget no deja el binario en `%LOCALAPPDATA%\<tool>\bin`, sino que crea
        // un enlace en su carpeta Links. Ahí está aunque el PATH aún no se haya
        // refrescado en este proceso.
        fallback: &[
            "LOCALAPPDATA/Microsoft/WinGet/Links/copilot.exe",
            "HOME/.local/bin/copilot",
            "/usr/local/bin/copilot",
        ],
        install_via: InstallVia::Cmd,
    },
    VibeTool {
        key: "06KiroCli",
        title: "Kiro CLI",
        // `--trust-all-tools` otorga confianza de sesión completa: equivale al
        // YOLO mode de Kiro CLI (documentado en kiro.dev/docs/cli/v3/permissions/).
        cli: "kiro-cli chat --trust-all-tools",
        // Se instala con el .exe de `https://kiro.dev` (o `winget`), no hay
        // comando de terminal: el instalador es un MSI/EXE con GUI. Por eso
        // `install` está vacío y `ensure_tools_installed()` no ofrece
        // instalarlo, sólo avisar de que falta.
        install: "",
        fallback: &["LOCALAPPDATA/Kiro-Cli/kiro-cli.exe"],
        install_via: InstallVia::Cmd,
    },
    VibeTool {
        key: "07Hermes",
        title: "Hermes",
        cli: "hermes",
        // Hermes arranca su TUI a pelo, sin flags: `hermes` a secas.
        install: "iex (irm https://hermes-agent.nousresearch.com/install.ps1)",
        // El README oficial dice que en Windows nativo vive bajo
        // `%LOCALAPPDATA%\hermes`; en Linux el instalador usa `~/.local/bin`.
        fallback: &[
            "LOCALAPPDATA/hermes/bin/hermes.exe",
            "HOME/.local/bin/hermes",
            "HOME/.hermes/bin/hermes",
            "/usr/local/bin/hermes",
        ],
        install_via: InstallVia::PowerShell,
    },
];

/// Resuelve el ejecutable real de una herramienta de Vibe Coding.
///
/// Primero busca en el PATH. Para `agy` (binario nativo, no npm) hace fallback a
/// las rutas de instalación conocidas, porque el instalador de Antigravity no
/// lo agrega al PATH por defecto.
///
/// En Linux la comprobación del PATH es `command -v` en vez del
/// `Get-Command` de PowerShell: con el comando de Windows el `if` nunca era
/// verdadero, así que `resolved` salía vacío y el menú se instalaba sin ninguna
/// sub-entrada.
fn resolve_tool_command(tool: &VibeTool) -> Option<(String, String)> {
    if command_exists(tool.cli.split_whitespace().next().unwrap_or(tool.cli)) {
        return Some((tool.cli.to_string(), String::new()));
    }

    // Fallback: ruta conocida de instalación de Antigravity CLI. En Windows es
    // `%LOCALAPPDATA%\agy\bin\agy.exe`, que no se agrega al PATH por defecto.
    // Segundo intento: la ruta fija donde el instalador deja el ejecutable cuando
    // no lo mete en el PATH. Se declara por herramienta en vez de codificar el
    // caso de Antigravity aquí, porque si no cada agente nuevo obliga a tocar
    // esta función.
    for candidate in fallback_candidates(tool) {
        if candidate.exists() {
            if let Some(p) = candidate.to_str() {
                return Some((p.to_string(), p.to_string()));
            }
        }
    }

    None
}

/// Rutas donde puede estar el ejecutable de `tool` sin estar en el PATH.
///
/// Los prefijos `LOCALAPPDATA/` y `HOME/` se expanden; lo que se escriba es
/// relativo a esas rutas, y una ruta absoluta se deja tal cual.
fn fallback_candidates(tool: &VibeTool) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();

    for hint in tool.fallback {
        if let Some(rest) = hint.strip_prefix("LOCALAPPDATA/") {
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                out.push(join_relative(std::path::Path::new(&local), rest));
            }
        } else if let Some(rest) = hint.strip_prefix("HOME/") {
            if let Some(home) = crate::utils::platform::home_dir() {
                out.push(join_relative(&home, rest));
            }
        } else {
            out.push(std::path::PathBuf::from(hint));
        }
    }

    out
}

/// Junta `base` con una ruta relativa escrita con `/`.
fn join_relative(base: &std::path::Path, relative: &str) -> std::path::PathBuf {
    relative
        .split('/')
        .filter(|s| !s.is_empty())
        .fold(base.to_path_buf(), |acc, part| acc.join(part))
}

/// Instala el menú cascada "Vibe Coding" con las sub-entradas que estén disponibles.
pub fn install_vibe_coding() -> Result<()> {
    if !crate::utils::platform::IS_WINDOWS {
        return super::context_menu_cosmic::install();
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
        println!("[WARN] No se encontró opencode, kilo, cline ni agy en el PATH.");
        println!("[WARN] Se instalará el menú Vibe Coding sin sub-entradas.");
        println!("[WARN] Instala las CLIs y vuelve a ejecutar el instalador.");
    } else {
        println!("[INFO] Detectadas {} herramienta(s) de Vibe Coding en el PATH.", resolved.len());
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
        return super::context_menu_cosmic::uninstall();
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

/// Detecta las herramientas de Vibe Coding que faltan y ofrece instalarlas.
///
/// Lo llama `install.bat` antes de instalar el menú: si el usuario acaba de
/// instalar una herramienta a mano, o si la va a instalar, el menú debe reflejar
/// las cuatro sub-entradas y no sólo las que ya estaban.
///
/// Pregunta una por una y **por omisión no instala**: instalar software es la
/// clase de cosa que no se hace sin consentimiento. El menú se puede instalar
/// igual sin ninguna de ellas, así que saltarse la instalación nunca bloquea.
pub fn ensure_tools_installed() -> Result<()> {
    let faltantes: Vec<&VibeTool> = VIBE_TOOLS
        .iter()
        .filter(|t| resolve_tool_command(t).is_none())
        .collect();
    let instalables: Vec<&VibeTool> = faltantes.iter().copied().filter(|t| !t.install.is_empty()).collect();

    if faltantes.is_empty() {
        println!("[OK] Las {} herramientas de Vibe Coding estan instaladas.", VIBE_TOOLS.len());
        return Ok(());
    }

    println!();
    println!("[INFO] Herramientas de Vibe Coding no encontradas: {}", faltantes.len());
    for tool in &faltantes {
        // Sin comando de instalación se indica el motivo de que no se ofrezca.
        let detalle = if tool.install.is_empty() {
            "instalador gráfico, no se instala desde la terminal".to_string()
        } else {
            tool.install.to_string()
        };
        println!("       - {}  ({})", tool.title, detalle);
    }
    if !instalables.is_empty() {
        println!();
        println!("[INFO] Se preguntará por {} de ellas.", instalables.len());
    }
    println!();

    for tool in &faltantes {
        println!("[?] {} no esta instalado.", tool.title);

        // Sin comando de instalación no hay nada que ejecutar: se instala a mano
        // con un instalador gráfico. Se avisa y se sigue.
        if tool.install.is_empty() {
            println!("    No se instala desde la terminal: usa su instalador.");
            continue;
        }

        println!("    Comando oficial: {}", tool.install);
        println!("    (npm global, instalador nativo o winget, segun la herramienta)");

        // El prompt no lleva `[s/N]`: `ask_text` ya imprime el default entre
        // corchetes. Con `None` no habría default, y entonces Enter devuelve ""
        // y la pregunta tampoco se podría responder con la barra espaciadora.
        let respuesta = ui::ask_text("    ¿Instalarlo ahora?", Some("n"))?;
        if !es_si(&respuesta) {
            println!("    [SKIP] No se instala {}.", tool.title);
            continue;
        }

        println!("    Instalando {}...", tool.title);
        match instalar(tool) {
            Ok(()) => {
                // Se comprueba con `resolve_tool_command` y no con
                // `command_exists` a secas: Antigravity se instala en
                // `%LOCALAPPDATA%\agy\bin`, fuera del PATH, así que en este
                // proceso `Get-Command agy` seguiría dando falso y se
                // reportaría como fallo siendo un éxito.
                if resolve_tool_command(tool).is_some() {
                    println!("    [OK] {} instalado.", tool.title);
                } else {
                    println!(
                        "    [WARN] {} se instaló pero su ejecutable no aparece.\n\
                         [WARN] Si es Antigravity, abre una terminal nueva: el PATH se\n\
                         [WARN] actualiza al iniciar sesion.",
                        tool.title
                    );
                }
            }
            Err(e) => println!("    [ERROR] No se pudo instalar {}: {e}", tool.title),
        }
    }

    Ok(())
}

/// Instala [`VibeTool::install`] con el shell que necesita.
fn instalar(tool: &VibeTool) -> Result<()> {
    match tool.install_via {
        InstallVia::Cmd => cmd::run_in_dir_status("cmd", &["/c", tool.install], &std::path::Path::new(".")),
        InstallVia::PowerShell => cmd::run_in_dir_status(
            "powershell",
            &["-NoProfile", "-Command", tool.install],
            &std::path::Path::new("."),
        ),
    }
}

/// ¿La respuesta es un sí?
///
/// Acepta lo que la gente escribe de verdad: `s`, `si`, `sí`, `y`, `yes`.
/// Todo lo demás (incluido el default y la cadena vacía) es no.
fn es_si(respuesta: &str) -> bool {
    matches!(
        respuesta.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_context_menu_is_ordered_after_vibe_and_quotes_both_paths() {
        assert!("zz_VibeCoding" < GITIGNORE_KEY);
        let exe = r"C:\Program Files\Compass\compass.exe";
        let folder = gitignore_context_command(exe, "%1");
        let background = gitignore_context_command(exe, "%V");
        assert!(folder.contains(r#""C:\Program Files\Compass\compass.exe""#));
        assert!(folder.contains(r#"--path "%1""#));
        assert!(background.contains(r#"--path "%V""#));
        assert!(folder.contains("cmd.exe /k"));
    }

    /// Los comandos de instalación están hardcodeados, así que un cambio de
    /// nombre en el paquete pasaría desapercibido hasta que alguien ejecutase
    /// el install y viera fallar la instalación. Estos valores son los de la
    /// documentación oficial de cada producto.
    #[test]
    fn install_commands_match_the_official_docs() {
        let esperado = [
            ("01OpenCode", "OpenCode", "npm install -g @opencode/cli", InstallVia::Cmd),
            ("02KiloCode", "Kilo Code", "npm install -g @kilocode/cli", InstallVia::Cmd),
            ("03Cline", "Cline", "npm install -g cline", InstallVia::Cmd),
            ("04Antigravity", "Antigravity", "irm https://antigravity.google/cli/install.ps1 | iex", InstallVia::PowerShell),
            (
                "05Copilot",
                "Copilot CLI",
                "winget install --id GitHub.Copilot --exact --accept-source-agreements --accept-package-agreements",
                InstallVia::Cmd,
            ),
            ("06KiroCli", "Kiro CLI", "", InstallVia::Cmd),
            ("07Hermes", "Hermes", "iex (irm https://hermes-agent.nousresearch.com/install.ps1)", InstallVia::PowerShell),
        ];

        assert_eq!(VIBE_TOOLS.len(), esperado.len(), "añade el test al añadir una herramienta");
        for (tool, (key, title, install, via)) in VIBE_TOOLS.iter().zip(esperado) {
            assert_eq!(tool.key, key);
            assert_eq!(tool.title, title);
            assert_eq!(tool.install, install);
            assert_eq!(tool.install_via, via);
        }
    }

    /// Kiro CLI se instala con un `.exe` (o `winget`), no con un comando de
    /// terminal. Por eso `install` va vacío, y el flujo de instalación **no debe
    /// preguntar** por él: preguntar algo a lo que no se puede responder con un
    /// Enter sería un bucle sin salida.
    #[test]
    fn a_empty_install_means_detect_only_never_ask() {
        let kiro = VIBE_TOOLS.iter().find(|t| t.key == "06KiroCli").unwrap();
        assert!(kiro.install.is_empty(), "Kiro CLI no debe ofrecer comando de instalación");
        assert!(
            !kiro.cli.is_empty(),
            "pero sí debe seguir teniendo un comando que ejecutar en el menú"
        );

        // Y la regla se sostiene en general: `install` vacío ⇒ no se pregunta.
        let sin_instalador: Vec<&str> = VIBE_TOOLS
            .iter()
            .filter(|t| t.install.is_empty())
            .map(|t| t.title)
            .collect();
        assert_eq!(sin_instalador, vec!["Kiro CLI"], "sólo Kiro CLI va sin instalador");
    }

    /// El ejecutable de Kiro CLI se llama `kiro-cli.exe`, no `kiro`: es lo que
    /// deja su instalador en `%LOCALAPPDATA%\Kiro-Cli`, y `resolve_tool_command`
    /// busca el primer token de `cli`.
    #[test]
    fn kiro_binary_name_matches_its_installer() {
        let kiro = VIBE_TOOLS.iter().find(|t| t.key == "06KiroCli").unwrap();
        let ejecutable = kiro.cli.split_whitespace().next().unwrap();
        assert_eq!(ejecutable, "kiro-cli");

        // Y la ruta de fallback tiene que apuntar a la carpeta real, que no
        // sigue el patrón `LOCALAPPDATA\<tool>\bin` del resto.
        assert!(
            kiro.fallback.contains(&"LOCALAPPDATA/Kiro-Cli/kiro-cli.exe"),
            "fallback de Kiro incorrecto: {:?}",
            kiro.fallback
        );
    }

    /// El prefijo numérico es lo único que fija el orden del submenú, porque
    /// Windows ordena las sub-claves de `shell` alfabéticamente. Si dos
    /// herramientas comparten número, el orden pedido se pierde.
    #[test]
    fn registry_keys_are_numbered_without_gaps_or_repeats() {
        let mut claves: Vec<&str> = VIBE_TOOLS.iter().map(|t| t.key).collect();
        assert_eq!(
            claves.len(),
            claves.iter().collect::<std::collections::HashSet<_>>().len(),
            "hay claves de registro repetidas: {claves:?}"
        );

        // `01OpenCode` → 1, `02KiloCode` → 2… y tienen que seguir en orden.
        let numeros: Vec<u32> = claves
            .iter()
            .map(|k| k.get(..2).unwrap_or("").parse::<u32>().unwrap_or_else(|_| panic!("{k} no empieza por dos dígitos")))
            .collect();
        let mut ordenados = numeros.clone();
        ordenados.sort_unstable();
        assert_eq!(numeros, ordenados, "VIBE_TOOLS no está en orden de submenú");
        assert_eq!(
            numeros,
            (1..=numeros.len() as u32).collect::<Vec<_>>(),
            "los prefijos deben ser 01, 02, 03… sin huecos: {numeros:?}"
        );

        claves.clear();
    }

    /// Los instaladores nativos dejan el binario fuera del PATH, y sin este
    /// fallback su sub-entrada no aparecería nunca en el menú aunque estuviera
    /// instalado. En Windows el prefijo `LOCALAPPDATA/` es el que importa.
    #[test]
    fn native_installers_declare_a_localappdata_fallback() {
        for tool in VIBE_TOOLS {
            let es_npm = matches!(tool.install, s if s.starts_with("npm "));
            if es_npm {
                assert!(
                    tool.fallback.is_empty(),
                    "{} se instala por npm, que sí agrega al PATH",
                    tool.title
                );
                continue;
            }
            assert!(
                tool.fallback.iter().any(|p| p.starts_with("LOCALAPPDATA/")),
                "{} es un instalador nativo y necesita fallback en %LOCALAPPDATA%: {:?}",
                tool.title,
                tool.fallback
            );
        }
    }

    /// `HOME/x` y `LOCALAPPDATA/x` tienen que expandirse; si no, `resolve_tool_command`
    /// compararía una ruta literal tipo `HOME/.local/bin/hermes` con el disco y
    /// nunca encontraría nada.
    #[test]
    fn fallback_hints_expand_to_real_paths() {
        let en_windows = VIBE_TOOLS
            .iter()
            .flat_map(|t| t.fallback.iter().map(move |p| (t.key, p)))
            .all(|(_, hint)| {
                hint.starts_with("LOCALAPPDATA/")
                    || hint.starts_with("HOME/")
                    || hint.starts_with('/')
            });
        assert!(en_windows, "todo fallback debe llevar prefijo conocido o ser absoluto");

        // Cada herramienta con fallback debe derivar al menos un camino real de
        // su ejecutable. Se usa `ends_with`, no igualdad, porque en Windows el
        // el candidato lleva `.exe` y en Linux no.
        for tool in VIBE_TOOLS.iter().filter(|t| !t.fallback.is_empty()) {
            let ejecutable = tool.cli.split_whitespace().next().unwrap();
            let candidatas = fallback_candidates(tool);
            assert!(
                candidatas.iter().all(|p| !p.to_string_lossy().contains("LOCALAPPDATA/")
                    && !p.to_string_lossy().contains("HOME/")),
                "{}: quedaron prefijos sin expandir: {candidatas:?}",
                tool.title
            );
            assert!(
                candidatas.iter().any(|p| {
                    let s = p.to_string_lossy().to_lowercase().replace('\\', "/");
                    s.ends_with(&format!("{ejecutable}.exe")) || s.ends_with(&format!("/{ejecutable}"))
                }),
                "{}: no se ha derivado ningún camino de {ejecutable}: {candidatas:?}",
                tool.title
            );
        }
    }

    /// `irm | iex` es PowerShell y no existe en `cmd`; al revés, `npm` en
    /// Windows es `npm.cmd` y su shim `npm.ps1` lo bloquea la ExecutionPolicy.
    /// Cada familia de instalador tiene que ir por su shell.
    #[test]
    fn every_install_command_uses_a_shell_that_understands_it() {
        for tool in VIBE_TOOLS {
            // Sin comando de instalación no hay shell que comprobar.
            if tool.install.is_empty() {
                continue;
            }

            let es_powershell = tool.install.contains("irm ") || tool.install.contains("iex");
            match tool.install_via {
                InstallVia::PowerShell => assert!(
                    es_powershell || !tool.install.contains("npm"),
                    "{} usa PowerShell pero el comando no lo necesita: {}",
                    tool.title,
                    tool.install
                ),
                InstallVia::Cmd => assert!(
                    tool.install.starts_with("npm ") || tool.install.starts_with("winget "),
                    "{} usa cmd, que sólo entiende bien npm o winget: {}",
                    tool.title,
                    tool.install
                ),
            }
        }
    }

    /// El nombre del comando que se busca en el PATH debe ser el primer token
    /// del `cli`: es lo que usa `resolve_tool_command` para detectarlo, así que
    /// si divergen la herramienta se instala pero el menú no la ofrece.
    ///
    /// Sólo se comprueba cuando hay instalador: Kiro CLI no tiene, y su binario
    /// (`kiro-cli`) no aparece en ningún comando de instalación.
    #[test]
    fn detected_command_matches_the_cli_of_the_menu() {
        for tool in VIBE_TOOLS {
            let ejecutable = tool.cli.split_whitespace().next().unwrap_or(tool.cli);
            assert!(!ejecutable.is_empty(), "{} no tiene comando", tool.title);

            if tool.install.is_empty() {
                continue;
            }
            assert!(
                tool.install.contains(ejecutable) || tool.install.contains("install"),
                "el instalador de {} no menciona {:?}",
                tool.title,
                ejecutable
            );
        }
    }

    #[test]
    fn only_yes_answers_are_taken_as_yes() {
        for afirmativo in ["s", "S", "si", "Sí", "sí", "y", "yes", "YES", " s ", "\tyes\n"] {
            assert!(es_si(afirmativo), "{afirmativo:?} debería contar como sí");
        }
        // El default de `ask_text` es "n", y con el stdin cerrado devuelve "".
        for negativo in ["", "n", "no", "N", "0", "1", "nop", "cancel", "maybe"] {
            assert!(!es_si(negativo), "{negativo:?} no debería contar como sí");
        }
    }
}

/// Flujo completo del instalador: preguntar por las que falten y montar el menú.
///
/// El menú se reinstala siempre al final, porque una herramienta recién
/// instalada tiene que aparecer como sub-entrada nueva aunque el menú ya
/// estuviera al día. `install_vibe_coding` es idempotente y se encarga de
/// preservar lo que haya.
pub fn setup() -> Result<()> {
    ensure_tools_installed()?;
    println!();
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
        let title = if crate::utils::platform::IS_WINDOWS {
            "MENÚ CONTEXTUAL DE WINDOWS"
        } else {
            "MENÚ CONTEXTUAL DE COSMIC FILES"
        };
        println!("╔══════════════════════════════════════════════════╗");
        println!("║     {title:<49}║");
        println!("╚══════════════════════════════════════════════════╝");
        println!();

        // En COSMIC, instalar y desinstalar afecta a la misma lista de presets
        // (`context_menu_cosmic`), así que ambas entradas comparten el estado.
        let cr_installed = if crate::utils::platform::IS_WINDOWS {
            is_installed("zz_Compass")
        } else {
            super::context_menu_cosmic::is_installed()
        };
        let vc_installed = if crate::utils::platform::IS_WINDOWS {
            is_installed(VIBE_KEY)
        } else {
            // Presencia de alguna entrada con el prefijo de Vibe Coding.
            super::context_menu_cosmic::has_vibe_entries()
        };

        let cr_label = if cr_installed { "✔ Compass" } else { "  Compass" };
        let vc_label = if vc_installed { "✔ Vibe Coding" } else { "  Vibe Coding" };

        // `options` y `actions` van en paralelo: el `match` de abajo despacha
        // por acción, no por el prefijo del texto. Antes se emparejaba con
        // `starts_with("2.")` y en Linux eso hacía que "2. Volver" instalara
        // Vibe Coding en lugar de volver.
        let mut options: Vec<String> = Vec::new();
        let mut actions: Vec<MenuChoice> = Vec::new();

        let push = |options: &mut Vec<String>, actions: &mut Vec<MenuChoice>, label: String, act: MenuChoice| {
            options.push(label);
            actions.push(act);
        };

        push(
            &mut options,
            &mut actions,
            format!(
                "1. {cr_label}  [{}]",
                if cr_installed { "Desinstalar" } else { "Instalar" }
            ),
            MenuChoice::ToggleCompass,
        );
        push(
            &mut options,
            &mut actions,
            format!(
                "2. {vc_label}  [{}]",
                if vc_installed { "Desinstalar" } else { "Instalar" }
            ),
            MenuChoice::ToggleVibeCoding,
        );
        push(&mut options, &mut actions, "3. Instalar todos".to_string(), MenuChoice::InstallAll);
        push(&mut options, &mut actions, "4. Desinstalar todos".to_string(), MenuChoice::UninstallAll);
        let back = options.len() + 1;
        push(&mut options, &mut actions, format!("{back}. Volver"), MenuChoice::Back);

        let choice = inquire::Select::new("Selecciona una opción:", options)
            .with_vim_mode(true)
            .raw_prompt()?;

        match actions.get(choice.index).copied().unwrap_or(MenuChoice::Back) {
            MenuChoice::ToggleCompass => {
                if cr_installed { uninstall()?; } else { install()?; }
                pause();
            }
            MenuChoice::ToggleVibeCoding => {
                if vc_installed { uninstall_vibe_coding()?; } else { install_vibe_coding()?; }
                pause();
            }
            MenuChoice::InstallAll => {
                install()?;
                install_vibe_coding()?;
                pause();
            }
            MenuChoice::UninstallAll => {
                uninstall()?;
                uninstall_vibe_coding()?;
                pause();
            }
            MenuChoice::Back => break,
        }
    }
    Ok(())
}

/// Acción del menú interactivo, emparejada 1:1 con cada opción mostrada.
#[derive(Debug, Clone, Copy)]
enum MenuChoice {
    ToggleCompass,
    ToggleVibeCoding,
    InstallAll,
    UninstallAll,
    Back,
}

// ── Helpers ────────────────────────────────────────────────────────────

/// ¿Está instalada la entrada `name`? `name` es `zz_Compass` o `VIBE_KEY`.
fn is_installed(name: &str) -> bool {
    // En Linux la fuente de verdad es la config de cosmic-files, no el registro.
    if !crate::utils::platform::IS_WINDOWS {
        return if name == VIBE_KEY {
            super::context_menu_cosmic::is_installed()
        } else {
            false
        };
    }

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

/// ¿Está `name` disponible como comando?
#[cfg(windows)]
fn command_exists(name: &str) -> bool {
    let ps = format!(
        "$null -ne (Get-Command -Name '{name}' -ErrorAction SilentlyContinue)"
    );
    let output = cmd::run("powershell", &["-NoProfile", "-Command", &ps]);
    output.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True").unwrap_or(false)
}

#[cfg(not(windows))]
fn command_exists(name: &str) -> bool {
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v -- {}", sh_quote(name))])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Entrecomilla para `sh` (comillas simples, escapando la interna).
#[cfg(not(windows))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
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
