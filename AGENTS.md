# Code Runner — Developer Guide

## Overview

`code-runner` es una herramienta CLI/TUI multiplataforma que automatiza tareas de desarrollo, compilación y configuración para proyectos **Tauri (Rust)**, **Flutter (Dart)**, **Kotlin Multiplatform (Compose)** y **Rust puro**. Detecta automáticamente el tipo de proyecto y adapta comandos, rutas y comportamientos.

## Project Types

| Tipo | Detección | Emoji | Menú |
|------|-----------|-------|------|
| `Tauri` | `pubspec.yaml` → Flutter, luego `tauri.conf.json` o `package.json` | 🦀 | Completo |
| `Flutter` | `pubspec.yaml` | 🎯 | Completo |
| `Kotlin` | `gradlew.bat` + `settings.gradle.kts` | 🔷 | Completo |
| `Rust` | `Cargo.toml` (después de todos los anteriores) | ⚙️ | Reducido (sin Android) |

El orden de detección es: **Flutter → Tauri → KMP → Rust**. Si un proyecto coincide con `package.json` pero no con `tauri.conf.json`, se clasifica como Tauri (para proyectos JS/Node con frontend).

## Platform Support

| Función | Windows | Linux (Pop!_OS / Ubuntu) |
|---------|---------|-------------------------|
| Entrada del launcher (`.desktop` + iconos) | ❌ | ✅ `install.sh` |
| Selector de proyecto (`--pick`) | ✅ | ✅ |
| Tauri dev/build (Windows + Android) | ✅ | ✅ |
| Rust puro | ✅ | ✅ |
| Android (dev + release APK/AAB) | ✅ | ✅ sólo dispositivo físico |
| Kotlin Multiplatform (Android + `packageDistributionForCurrentOS`) | ✅ | ✅ |
| Flutter Android (APK/AAB) | ✅ | ✅ |
| Flutter **desktop** | ✅ | ✅ `-d linux` / `flutter build linux` |
| Tauri **desktop** | ✅ (exe/msi) | ✅ AppImage/deb (`platform::desktop_artifact_exts()`) |
| KMP **desktop** | ✅ (exe) | ✅ binario sin extensión (mismo helper) |
| Menú contextual Code Runner + Vibe Coding | ✅ registro | ⚠️ COSMIC Files ≥ 1.0.10, entradas **planas** (ver abajo) |
| Configurar Terminal (PSReadLine) | ✅ | ❌ oculto (perfil de PowerShell) |

### Vibe Coding en COSMIC Files (`context_menu_cosmic.rs`)

COSMIC 1.0.10 (mayo 2026) añadió *user-defined context menu actions*. Se configuran como presets `ContextActionPreset` en el campo `context_actions` de la config de cosmic-files.

**Ruta** — cosmic-files usa `cosmic_config::Config::new(App::APP_ID, CONFIG_VERSION)`, que resuelve como `$XDG_CONFIG_HOME/cosmic/<APP_ID>/v<VERSION>/<campo>`. Con `APP_ID = "com.system76.CosmicFiles"` y `CONFIG_VERSION = 1`:

```
$XDG_CONFIG_HOME/cosmic/com.system76.CosmicFiles/v1/context_actions
```

⚠️ El APP_ID lleva punto y mayúsculas. **No** es `cosmic-files`: escribir en `cosmic-files/` deja el fichero donde cosmic-files nunca lo busca, y el síntoma es que el menú simplemente no aparece — sin error en la app, sin error en los logs, nada.

**Formato: RON, no JSON.** cosmic-config serializa con `ron`. De ahí la dependencia `ron` en `Cargo.toml`; `serde_json` no sirve aquí.

```ron
[
    (
        name: "Vibe Coding: OpenCode",
        confirm: false,
        selection: Folders,
        steps: [
            "/ruta/al/wrapper.sh \"%f\"",
        ],
    ),
]
```

- `selection`: enum `Any` | `Files` | `Folders`, con alias de serde en minúsculas.
- `steps`: se parsean con `mime_app::exec_to_command`, o sea **sintaxis `Exec=` de freedesktop**. El `%f` debe ir entrecomillado: se sustituye literalmente, y sin comillas una ruta con espacios rompe el tokenizado.
- **Un wrapper `vibe-<slug>.sh` por herramienta** en `$XDG_DATA_HOME/code-runner/vibe/`, en vez de incrustar `sh -c "cd %f && …"`. El `cd` y el quoting viven en el archivo (se escapan una vez); el preset sólo invoca el wrapper.
- **Limitación: sin submenús anidados.** `ContextActionPreset` sólo tiene `name`, `confirm`, `selection`, `steps` — no hay campo de agrupación. Las 4 entradas quedan **planas**, no en cascada como en Windows. El prefijo `"Vibe Coding: "` las agrupa visualmente y es el criterio que usa `uninstall()` para borrarlas sin tocar las del usuario.
- `install()` hace **merge**: preserva los presets previos del usuario, y sólo reemplaza los que empiezan con el prefijo. Respalda el archivo previo a `context_actions.ron.code-runner-bak` antes de escribir.
- Requiere cerrar y reabrir COSMIC Files. Sin `cosmic-files` en el PATH, `install()` falla con mensaje explícito.
- `VIBE_TOOLS` está duplicado (el de `context_menu.rs` usa `key` de registro, que aquí no aplica). Si se unifica, debe subir a un módulo común.

### Launcher de COSMIC (`.desktop`)

`install.sh` genera `~/.local/share/applications/code-runner.desktop` e instala los
PNG en `~/.local/share/icons/hicolor/{16,24,32,48,64,128}x*/apps/`.

- **El `.ico` no le sirve al launcher.** Pop!_OS no trae ImageMagick ni
  `icoutils`, así que `tools/ico2png.py` (solo stdlib) decodifica el BMP-DIB de
  32bpp de `Icon.ico` y genera el set de PNG. Ejecuta `python3 tools/ico2png.py`
  tras cambiar el icono original.
- **`code-runner` es una TUI: el launcher no puede lanzarla directamente** (no
  hay tty). El `Exec` envuelve el binario en el emulador detectado:
  `cosmic-term --title "Code Runner" -e "<exe>" --pick`.
- **`--pick` es obligatorio en el `.desktop`**: desde el launcher el directorio
  actual es `$HOME`, donde no hay proyecto que autodetectar. Sin `--pick`,
  `detect()` fallaría y la app no abriría.
- El `Exec` se valida con `desktop-file-validate` si está disponible; una entrada
  mal formada desaparece del launcher en silencio.
- La tabla de terminales está **duplicada** entre `install.sh` y `TERMINALS` en
  `cmd.rs` (`lead`/`title_flag`/`exec_flag`). Si se unifica, ha de incluir el
  `lead` de wezterm (`start --`), que no admite `--title`.

### Selector de proyecto (`context::pick_project_root`)

Navegador de directorios que se usa cuando `--pick` está activo o cuando
`detect()` falla y no hay `--task`. `main.rs` cae ahí automáticamente: es lo que
hace utilizable la entrada del launcher.

- Mantiene `options` y `actions` en vectores paralelos; despacha por acción, no
  por el prefijo del texto (un `starts_with("2."")` ya causó un bug: en Linux
  "2. Volver" instalaba Vibe Coding).
- Las subcarpetas se marcan con su tipo (`classify_dir`), para que se sepa qué
  es un proyecto antes de entrar.
- Los ocultos se listan al final, no se ocultan.
- Esc / Ctrl-C devuelven `Ok(None)`; `main.rs` sale sin error.

### Notas de implementación Linux

- **Artefactos de desktop sin extensión**: `platform::desktop_artifact_exts()`
  devuelve `["exe","msi"]` en Windows y `[]` en Linux. `sh_newest_artifact`
  interpreta la lista vacía como "cualquier archivo regular", no como "nada que
  buscar": con `["exe"]` en Linux, `reveal_on_success` nunca encontraba el
  binario (que no lleva extensión) y siempre caía al fallback.
- **Wrappers de Vibe Coding**: abren su propia terminal. cosmic-files lanza el
  paso sin tty, así que un `exec opencode` directo vuelca secuencias ANSI a un
  pipe. `cmd::terminal_prefix()` construye el argv; se comparte con
  `spawn_in_terminal` para que ambos coincidan en la forma de invocar el emulador.
- **Detección de CLIs en `command_exists`**: en Linux es `command -v`, no el
  `Get-Command` de PowerShell (que siempre daba falso ⇒ menú sin sub-entradas).
- **Rutas**: todos los literales de ruta en el crate usan `/` como separador. Windows lo acepta y `std::path` lo normaliza, y en Linux el `\` se trataría como carácter literal de nombre de archivo (haría que `clean_project` reportara `[OK]` sin borrar nada). `src/utils/platform.rs` cubre los casos dinámicos.
- **Ejecutables**: `platform::exe_name("adb")` → `adb.exe` en Windows, `adb` en Linux. Lo usan `adb`, `emulator`, `sdkmanager` y `keytool`.
- **Gradle**: `platform::find_gradlew()` acepta `gradlew` **y** `gradlew.bat`, porque un proyecto KMP clonado entre Windows y Linux conserva sólo uno de los dos wrappers. `ensure_gradle()` abortaba en Linux antes de hacer nada. `gradlew_cmd(ctx, args)` usa el wrapper que exista de verdad (y ejecuta `gradlew.bat` con `sh` si es el único presente); también da `+x` a `gradlew` cuando falta, porque git no siempre lo preserva al clonar entre sistemas.
- **ANDROID_HOME**: se resuelve por `ANDROID_HOME` → `ANDROID_SDK_ROOT` → `$HOME/Android/Sdk` → `$HOME/android-sdk` → `$XDG_DATA_HOME/Android/Sdk` → `/usr/lib/android-sdk`.
- **Ventanas**: `spawn_new_window*()` detecta un emulador de terminal (`cosmic-term`, `gnome-terminal`, `konsole`, `xfce4-terminal`, `alacritty`, `kitty`, `wezterm`, `xterm`) y ejecuta el comando con `sh -lc`. Antes, la rama `#[cfg(not(windows))]` seguía invocando `powershell`, lo que dejaba la TUI inutilizable.
- **Builds**: `reveal_on_success()` genera `sh` en Linux (`find -printf` + `sort -rn` para el artefacto más reciente, `mv` para renombrar, `xdg-open`/`gio open` para revelar). Mantiene la ventana abierta con `read -r` en ambos casos.
- **Sintaxis de shell embebida**: `TAURI_DEV_HOST`, `export RELEASE_*` y `ANDROID_SERIAL` se emiten con sintaxis `set X=Y &` / `$env:X=` / `$LASTEXITCODE` en Windows y `X=Y` / `export X=` / `$?` en Linux.
- **PowerShell call-operator**: `menu.rs` omite el `&` al construir el comando en Linux (en `sh` sería un error de sintaxis).
- **Instalador**: `install.sh` (Linux) reemplaza a `install.bat` (Windows). No instala menús contextuales.
- **Paridad pendiente** (fuera del MVP): targets Linux de Flutter y Tauri desktop (comandos y `exts` de artefactos), `find_keytool` ya cubre `/usr/lib/jvm` pero `debug()` de Android en Linux usa `chrome://inspect` en vez de `edge://inspect`.

## ProjectContext

Estructura inyectada en todas las funciones de tarea:

```
ProjectContext
  script_dir: PathBuf           # Raíz del proyecto detectado
  project_type: ProjectType     # Tauri | Flutter | Kotlin | Rust
  android_home: Option<PathBuf>
  ndk_home: Option<PathBuf>
  adb_path: Option<PathBuf>
  sdk_manager: Option<PathBuf>
  has_node_modules: bool
  has_android_project: bool     # false para Rust
  has_whatsapp_engine: bool     # engine_dir existe
```

## Module Map

```
main.rs
  └─ Cli (clap) → --task, --path
      ├─ sin args → menu::run()
      └─ con args → run_task_direct() dispatches by task name
          └─ run_global_task() para context-menu y opencode

menu.rs
  ├─ MenuItem (20 variants: WindowsDev, AndroidDev, ..., InstallDebug, Sha1, Version, ContextMenu)
  ├─ ratatui TUI con cards en grilla 3-columnas, 3 secciones (DEV/MNT/CFG)
  ├─ Filtro por tipo de proyecto (Rust oculta Android/Signing/Keystore)
  ├─ handle_event(): devuelve MenuAction (Run | Quit); teclado + mouse (grid matemático)
  ├─ Sin botón Salir: `Esc` cierra el programa (`MenuAction::Quit`)
  └─ run(): loop: draw → handle → restore → execute → re-enter

utils/
  ├─ context.rs — detect(), ProjectType, ProjectContext, whatsapp_path()
  ├─ platform.rs — helpers de portabilidad: IS_WINDOWS, exe_name(), gradlew_name(), home_dir(), data_dir(), find_gradlew(), NDK_VERSION
  ├─ cmd.rs — run*, spawn_new_window, spawn_new_window_autoexit, spawn_new_window_interactive, run_npm_install, clear_screen, remove_dir, reveal_on_success
  ├─ ui.rs — ask_text(), ask_password(), success_banner(), info_banner(), pause()
  └─ android.rs — select_device(), wait_for_device_boot(), list_physical_devices() [sólo dispositivos físicos]

tasks/
  ├─ windows.rs — dev(), build()                         [Tauri only]
  ├─ android.rs — dev(), build_arm64(), build_all(), debug(), configure_signing(), ensure_package_consistent() [verifica identifier vs directorio paquete Android]
  ├─ flutter.rs — run_windows(), run_android(), profile_windows(), profile_android(), build_*(), clean()
  ├─ kotlin.rs  — run_windows() [con hot reload], run_android(), install_debug() (`:<modulo>:installDebug` + lanza con `am start`), build_*(), clean(); deteccion de modulo Gradle `:app`/`:composeApp` via `gradle_module()`; release_signing_env() [exporta RELEASE_* vars para Gradle]
  ├─ rust.rs    — run(), build(), clean()                [Rust only]
  ├─ cleanup.rs — clean_cache() [multi-type], clean_project() [Tauri only]
  ├─ git.rs     — init()
  ├─ gitignore.rs — create() [templates por tipo, incluye Rust]
  ├─ keystore.rs — manage() [Tauri: menú backup/restore]
  ├─ signer.rs  — create_or_import(), patch_build_gradle()
  ├─ sha1.rs    — show() [huellas SHA-1/SHA-256 debug+release, lee keystore.properties/key.properties]
  ├─ terminal.rs — configure() [PSReadLine profile]
  ├─ context_menu.rs — install/uninstall/toggle para CodeRunner + Vibe Coding (menú cascada con ExtendedSubCommandsKey: OpenCode, Kilo Code, Cline, Antigravity)
  ├─ context_menu_cosmic.rs — install/uninstall para COSMIC Files (context_actions, COSMIC ≥ 1.0.10)
  └─ whatsapp.rs — build()
  └─ version.rs — bump() [port de versionar.bat: Tauri/Rust, todo-o-nada]
```

## Task Routing

### Por tipo de proyecto

| Menu Item | Tauri | Flutter | Kotlin | Rust |
|-----------|-------|---------|--------|------|
| WindowsDev | `windows::dev` | `flutter::run_windows` | `kotlin::run_windows` | `rust::run` |
| AndroidDev | `android::dev` | `flutter::run_android` | `kotlin::run_android` | oculto |
| FlutterProfile | oculto | `flutter::profile_windows` / `flutter::profile_android` (pregunta) | oculto | oculto |
| WindowsBuild | `windows::build` | `flutter::build_windows` | `kotlin::build_windows` | `rust::build` |
| AndroidBuildArm64 | `android::build_arm64` | `flutter::build_apk` | `kotlin::build_apk` (assembleRelease) | oculto |
| AndroidBuildAll | `android::build_all` | `flutter::build_aab` | `kotlin::build_aab` (bundleRelease) | oculto |
| InstallDebug | oculto | oculto | `kotlin::install_debug` (`:<modulo>:installDebug` + lanza) | oculto |
| CleanCache | `cleanup::clean_cache` | `cleanup::clean_cache` | `cleanup::clean_cache` | `cleanup::clean_cache` |
| DebugAndroid | `android::debug` | mensaje info | mensaje info | oculto |
| ConfigureSigning | `android::configure_signing` | `android::configure_signing` | `android::configure_signing` | oculto |
| Sha1 | `sha1::show` | `sha1::show` | `sha1::show` | oculto |
| Version | `version::bump` (pkg/cargo/tauri) | mensaje info | mensaje info | `version::bump` (Cargo.toml) |
| CleanProject | `cleanup::clean_project` | `flutter::clean` | `kotlin::clean` | `rust::clean` |
| CreateGitignore | `gitignore::create` | `gitignore::create` | `gitignore::create` | `gitignore::create` |
| InitGit | `git::init` | `git::init` | `git::init` | `git::init` |
| ManageKeystore | `keystore::manage` | `android::configure_signing` | `android::configure_signing` | oculto |
| BuildWhatsApp | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` |
| ConfigureTerminal | `terminal::configure` | `terminal::configure` | `terminal::configure` | `terminal::configure` |
| InstallContextMenu | `context_menu::toggle` | `context_menu::toggle` | `context_menu::toggle` | `context_menu::toggle` |
| Salir | `Esc` → `process::exit(0)` | ídem | ídem | ídem |

### Tareas globales (sin projecto)

| Task | Ruta |
|------|------|
| `context-menu` / `opencode` | `context_menu::toggle()` (menú interactivo) |
| `context-menu-install` | `context_menu::install()` |
| `context-menu-uninstall` | `context_menu::uninstall()` |
| `opencode-install` | `context_menu::install_opencode()` → `install_vibe_coding()` |
| `opencode-uninstall` | `context_menu::uninstall_opencode()` → `uninstall_vibe_coding()` |
| `cline-install` | `context_menu::install_cline()` → `install_vibe_coding()` |
| `cline-uninstall` | `context_menu::uninstall_cline()` → `uninstall_vibe_coding()` |
| `kilo-install` | `context_menu::install_kilocode()` → `install_vibe_coding()` |
| `kilo-uninstall` | `context_menu::uninstall_kilocode()` → `uninstall_vibe_coding()` |
| `antigravity-install` | `context_menu::install_antigravity()` → `install_vibe_coding()` |
| `antigravity-uninstall` | `context_menu::uninstall_antigravity()` → `uninstall_vibe_coding()` |

## Path Conventions

### Android signing

| Type | Android dir | Props file |
|------|------------|------------|
| Tauri | `src-tauri/gen/android/app/` | `keystore.properties` |
| Flutter | `android/app/` | `key.properties` |
| Kotlin | `app/` o `composeApp/` (detección automática) | `keystore.properties` |
| Rust | N/A | N/A |

### WhatsApp engine

| Type | Engine dir | Bundle output |
|------|-----------|--------------|
| Tauri | `src-tauri/whatsapp-engine/` | `src-tauri/resources/whatsapp-engine-bundle/` |
| Flutter | `whatsapp_engine/` | `resources/whatsapp-engine-bundle/` |
| Kotlin | `whatsapp_engine/` | `composeApp/resources/whatsapp-engine-bundle/` |
| Rust | `whatsapp_engine/` | `resources/whatsapp-engine-bundle/` |

### Android project detection

| Type | Path checked |
|------|-------------|
| Tauri | `src-tauri/gen/android/gradlew.bat` |
| Flutter | `android/gradlew.bat` |
| Kotlin | `gradlew.bat` (raíz) |
| Rust | N/A (siempre `false`) |

## Context Menu (Windows Registry)

### Code Runner (entrada simple)

| Entrada | Comando | Clave Registro |
|---------|---------|----------------|
| **Code Runner** | `"<exe_path>" "%1"` | `HKCR\Directory\shell\zz_CodeRunner` |

### Vibe Coding (menú cascada: OpenCode + Kilo Code + Cline + Antigravity)

Usa el método **`ExtendedSubCommandsKey`** (el oficial de Windows para menús cascada, mismo que usa Command Prompt y PowerShell).

**Estructura de registro:**
```
HKCU\Software\Classes\Directory\shell\zz_VibeCoding
  MUIVerb = "Vibe Coding"
  Icon = "path\to\opencode.ico"
  ExtendedSubCommandsKey = "Directory\ContextMenus\VibeCoding"

HKCU\Software\Classes\Directory\ContextMenus\VibeCoding\shell
  01OpenCode     → command
  02KiloCode     → command
  03Cline        → command
  04Antigravity  → command
```

**Sub-entradas (en este orden):**

| # | Sub-entrada | Comando | Descripción |
|---|-------------|---------|-------------|
| 01 | **OpenCode** | `cmd.exe /c start "OpenCode" /D "%V" cmd.exe /c "opencode --auto"` | Abre OpenCode en la carpeta |
| 02 | **Kilo Code** | `cmd.exe /c start "Kilo Code" /D "%V" cmd.exe /c "kilo"` | Abre Kilo Code en la carpeta |
| 03 | **Cline** | `cmd.exe /c start "Cline" /D "%V" cmd.exe /c "cline -i"` | Abre Cline (`-i` evita modo headless) |
| 04 | **Antigravity** | `cmd.exe /c start "Antigravity" /D "%V" cmd.exe /c "agy"` | Abre Antigravity CLI (Google) |

**Notas técnicas:**
- **Orden del submenú**: Windows ordena las sub-claves de `shell` **alfabéticamente**. Por eso las claves llevan prefijo numérico (`01OpenCode`, `02KiloCode`, ...) para forzar el orden requerido en `VIBE_TOOLS` (src/tasks/context_menu.rs). Agregar una herramienta = agregar una entrada al final de `VIBE_TOOLS` con prefijo incremental.
- `%V` = directorio de la carpeta seleccionada (funciona con `ExtendedSubCommandsKey`; `%1` NO funciona).
- La entrada padre usa `MUIVerb` en vez de `(default)` para el texto visible.
- Las sub-entradas están en `Directory\ContextMenus\VibeCoding\shell\` (ruta compartida para `Directory` y `Directory\Background`).
- Se registran para **carpeta** (`Directory`) y **fondo** (`Directory\Background`).
- `remove_vibe_ctx_tree()` borra el árbol `ContextMenus\VibeCoding` completo en cada instalación — evita claves huérfanas de versiones previas que alteraban el orden.
- Se limpian entradas viejas individuales (`zz_OpenCode`, `zz_pCline`, `zz_pKilo`) al instalar.
- **Antigravity CLI**: binario nativo (no npm) instalado por `irm https://antigravity.google/cli/install.ps1 | iex` → `%LOCALAPPDATA%\agy\bin\agy.exe`. No siempre está en PATH; `resolve_tool_command()` hace fallback a esa ruta si `Get-Command agy` falla.
- `install.bat` instala/actualiza Vibe Coding con detección de "ya instalado".
- El menú interactivo `toggle()` gestiona Code Runner + Vibe Coding + "Instalar todos" / "Desinstalar todos".
- **Idempotencia**: `install()` compara tanto el comando (`command_is_current()`) como el texto visible (`label_is()`), así un cambio de nombre fuerza el reinstall aunque el comando ya sea correcto.
- **Los cambios del registro requieren reiniciar Explorer**: `taskkill /f /im explorer.exe & start explorer.exe`.

## Key Patterns

### New window execution
Tareas largas se lanzan en **nueva ventana** con `CREATE_NEW_CONSOLE`. Tres variantes:

| Variante | Interno | Uso |
|----------|---------|-----|
| `spawn_new_window()` | `powershell -NoExit -Command` | Windows Build (no interactivo, ventana se queda abierta) |
| `spawn_new_window_autoexit()` | `powershell -Command` | Menú TUI (sin `-NoExit`, ventana se cierra al terminar) |
| `spawn_new_window_interactive()` | `cmd /c "title & command & pause"` | Dev servers interactivos (`flutter run`, `gradlew :composeApp:run`, `npx tauri android dev`). Usa `cmd` en vez de PowerShell porque este último no reenvía correctamente el teclado a procesos hijo en modo `-Command`. |

La TUI retorna inmediatamente en todos los casos.

### Reveal build artifact + auto-close (`cmd::reveal_on_success`)
Todos los builds **release** envuelven su comando con `cmd::reveal_on_success(command, folder, exts)`:
encadena `<comando>; if ($?) { ... explorer.exe /select,<artefacto> ...; exit } else { ... Read-Host; exit }`.
Al terminar con éxito abre el Explorador resaltando el artefacto más reciente (`exe/msi/apk/aab`)
dentro de `folder` (ruta absoluta derivada de `ctx.script_dir`) y **la ventana de build se cierra sola** (`exit`).
Si el build falla, la ventana muestra `[ERROR]` y espera una tecla antes de cerrarse (para poder leer el error).
Las tareas de build ya no llaman a `pause()`/`ui::pause()`, por lo que la ventana intermedia de `code-runner --task` también se cierra sola.

| Task | folder | exts |
|------|--------|------|
| `windows::build` (Tauri) | `src-tauri/target/release/bundle` | exe, msi |
| `android::{build_arm64,build_armv7,build_all}` | `src-tauri/gen/android/app/build/outputs/apk` | apk |
| `flutter::{build_windows}` | `build/windows/x64/runner/Release` | exe |
| `flutter::{build_apk}` | `build/app/outputs/flutter-apk` | apk |
| `flutter::{build_aab}` | `build/app/outputs/bundle/release` | aab |
| `kotlin::{build_windows}` | `composeApp/build/compose/binaries/main` | exe |
| `kotlin::{build_apk}` | `<modulo>/build/outputs/apk/release` | apk |
| `kotlin::{build_aab}` | `<modulo>/build/outputs/bundle/release` | aab |
| `rust::build` | `target/release` | exe |

El filtrado usa `Get-ChildItem -LiteralPath ... -Recurse -File | Where-Object { $_.Extension -in ... }`
+ `Sort-Object LastWriteTime -Descending` (robusto a nombres de producto desconocidos y a `[` en rutas).
Si no encuentra artefacto, abre la carpeta `folder` directamente (en vez de nada).

### Rename build artifact (nombre + versión)
Todos los builds **release** de Android (APK/AAB) renombran el artefacto resultante a `<AppName>-v<version>.<ext>` usando `reveal_on_success(..., Some("AppName-v1.0.0"))`.

| Fuente | Nombre | Versión | Ejemplo |
|--------|--------|---------|---------|
| Tauri | `tauri.conf.json` → `productName` | `version` | `School_Tracker_Pro-v1.1.0.apk` |
| Flutter | `pubspec.yaml` → `name` | `version` (sin `+build`) | `my_app-v2.0.0.aab` |
| Kotlin | `build.gradle.kts` → `versionName` | `res/values/strings.xml` → `app_name` | `SongSync-v1.0.0.apk` |

El sanitizado reemplaza espacios y caracteres especiales por `_`. Los builds de **Windows** (exe/msi) y **Rust** NO renombran (el nombre del producto ya viene del build).

**Importante**: la llamada a Explorer es `explorer.exe ("/select,`"" + $f.FullName + "`"")` — el path debe ir
**entre comillas dobles embebidas** (`/select,"ruta con espacios"`). `explorer.exe /select,<path>` sin esas comillas
falla el parser de Explorer cuando la ruta tiene espacios y abre **Documentos** en su lugar.

### npm en Windows
`cmd::run_npm_install()` usa `cmd /c npm.cmd install` — necesario porque `npm` en Windows es realmente `npm.cmd`.

### Gradle en Windows (KMP)
Los comandos de Gradle se ejecutan como `cmd /c gradlew.bat <args>` porque PowerShell no puede ejecutar `.bat` directamente.

### KMP Desktop (Hot Reload)
`kotlin::run_windows()` ejecuta `:composeApp:run` con Hot Reload incorporado.

### Android signing
`signer::patch_build_gradle()` soporta tanto `build.gradle.kts` (Kotlin Script) como `build.gradle` (Groovy), inyectando:
1. Properties loader (variables desde `keystore.properties`/`key.properties`)
2. `signingConfigs { release { ... } }`
3. `signingConfig` en build type release

Para Kotlin release builds (`build_apk`/`build_aab`), el tool también exporta las variables de entorno `RELEASE_*` que many `build.gradle.kts` usan en su `signingConfigs`, permitiendo que la firma funcione sin modificar el archivo del proyecto.

### PowerShell registry (PS v5.1)
- Usar `-Type` en lugar de `-PropertyType` (este último solo funciona en PowerShell Core).
- `Test-Path -PathType Leaf` no funciona en claves de registro; se omite.

### Directory removal (Windows)
`cmd::remove_dir()` usa `powershell Remove-Item -Recurse -Force` para rutas largas que `std::fs::remove_dir_all` no puede manejar.

### TUI Menu — Ventana separada
Todas las tareas del menú TUI se lanzan en una **nueva ventana PowerShell** ejecutando `code-runner --task <task> --path "<proyecto>"`. El menú principal permanece abierto. La ventana secundaria se cierra al presionar Enter.

### Android dev (Tauri)
`android::dev()` corre `npx tauri android dev` directamente. Antes ejecuta:
1. `ensure_node_modules()` — npm install si falta `node_modules/`
2. `ensure_android_project()` — `npx tauri android init` si falta `src-tauri/gen/android/`
3. `ensure_package_consistent()` — verifica que el directorio del paquete Android (`app/src/main/java/<identifier>`) coincida con el `identifier` de `tauri.conf.json`. Si falta, ofrece eliminar `src-tauri/gen/android` y recrear con `tauri android init`.
4. `select_device()` — elige dispositivo (ver sección Device Selection)
5. NDK / Rust targets según sea necesario

### Kotlin release builds (APK/AAB)
`kotlin::build_apk()` ejecuta `:assembleRelease` (antes `:assembleDebug`) y `kotlin::build_aab()` ejecuta `:bundleRelease`. Ambos leen `app/keystore.properties` (o `composeApp/keystore.properties`) y exportan variables de entorno para Gradle:
- `RELEASE_STORE_FILE` (ruta absoluta al keystore)
- `RELEASE_STORE_PASSWORD`
- `RELEASE_KEY_ALIAS`
- `RELEASE_KEY_PASSWORD`

Esto permite que el `signingConfigs` existente en `build.gradle.kts` (que usa variables de entorno `RELEASE_*`) firme automáticamente sin modificar el archivo del proyecto. Si el archivo de propiedades no existe, avisa con `[WARN]` y continúa (el build puede fallar si no hay firma configurada).

### Tauri Android package consistency (`ensure_package_consistent`)
Detecta el error típico "Project directory ... does not exist" que ocurre cuando se cambia el `identifier` en `tauri.conf.json` sin regenerar el proyecto Android. Lee el identifier, calcula la ruta esperada del paquete Java (`app/src/main/java/<pkg_path>`) y si no existe:
1. Muestra el identifier esperado y la ruta faltante.
2. Pregunta si desea eliminar `src-tauri/gen/android` y ejecutar `tauri android init`.
3. Si responde `s`: elimina y recrea. Si responde `n`: continúa (el build fallará).

### Device Selection (`android::select_device()`)

**Sólo dispositivos físicos.** El emulador se eliminó del flujo: ya no hay
tarjeta "Emulator", ni task `launch-emulator`, ni `emulator_path` en el contexto,
ni AVDs. `utils/android.rs` se reduce a `Device { serial }` + `select_device()`.

| Escenario | Comportamiento |
|-----------|---------------|
| 1 físico conectado | Auto-selecciona, espera `sys.boot_completed` |
| > 1 físicos | Menú con los físicos, luego espera el boot del elegido |
| 0 físicos | Error con pasos concretos (debug USB, huella RSA, `adb devices -l`) |

Dos detalles que importan en la práctica:

- **`emulator-*` se filtra** por serie. Si un emulador queda corriendo de antes,
  no se confunde con un teléfono.
- **Espera de boot**: un teléfono recién conectado aparece como `device` en
  `adb devices` pero aún rechaza `adb install`. `wait_for_device_boot()` espera
  a que `sys.boot_completed` valga `1` (60 s) para fallar tarde, no con un error
  confuso en medio del build.
- **`unauthorized` / `offline`**: cuando hay líneas en `adb devices` pero ningún
  dispositivo utilizable, se diagnostica el caso concreto en vez de decir
  genéricamente "no se detectaron dispositivos".

### UI Helpers (`ui.rs`)
| Función | Descripción |
|---------|-------------|
| `ask_text(prompt, default)` | Prompt inline (cursor después de `: `), no usa inquire |
| `ask_password(prompt)` | Password inline con enmascaramiento `*`, raw mode vía crossterm |
| `success_banner(title)` | Borde ┌─┐└─┘ con ✅ centrado |
| `info_banner(title)` | Borde ┌─┐└─┘ con ℹ️ centrado |
| `pause()` | Espera Enter (read_line, sin raw mode) |

### Signer prompts
`signer.rs` usa `ui::ask_text()` y `ui::ask_password()` en vez de `inquire::Text`/`inquire::Password` para que el cursor aparezca después de los dos puntos en la misma línea.

### CLI directo (sin TUI)
```bash
code-runner --task windows-dev
code-runner --path /ruta/proyecto --task sign
code-runner --task context-menu-install
code-runner  # abre menú interactivo
```

## .gitignore Templates

Cada tipo de proyecto tiene su propio `.gitignore`:

- **Rust**: ignorar target/ (preserva src/, Cargo.toml, tests/)
- **Tauri**: ignorar Rust target, gen/android, node_modules, dist, Vite cache
- **Flutter**: ignorar build, .dart_tool, Android builds, ios symlinks
- **Kotlin**: ignorar build, .gradle, .kotlin, local.properties, APK/AAB artefactos

Todos preservan: fuente, config, wrapper/scripts, firmas, motor WhatsApp.

## install.bat

Script de instalación para Windows:

1. Compila el proyecto con `cargo install --path .`
2. **Siempre** copia `assets/icon/Icon.ico` a `%USERPROFILE%\.config\code-runner\Default.ico`
3. **Siempre** copia `assets/icon/opencode.ico` al mismo directorio
4. Instala los 4 menús contextuales (Code Runner, OpenCode, Cline, Kilo Code) con detección de "ya instalado" para cada uno:
   - `code-runner --task context-menu-install`
   - `code-runner --task opencode-install`
   - `code-runner --task cline-install`
   - `code-runner --task kilo-install`
5. Si `cargo install` falla, muestra advertencia pero no detiene la copia de iconos ni la instalación de menús

## install.sh

Script de instalación para Linux (Pop!_OS / Ubuntu). Diferencias con `install.bat`:

1. Hace `cd` a su propio directorio (el check de `Cargo.toml` miraba el CWD de
   quien lo invocaba, y fallaba con `./ruta/al/proyecto/install.sh`)
2. Verifica `command -v cargo` en vez de `where cargo`
3. Cierra instancias con `pkill -x code-runner` en vez de `taskkill`
4. Compila con `cargo install --path . --force`
5. Instala la **entrada del launcher** (`.desktop`) e iconos PNG en hicolor
6. Comprueba SDK, **JDK** y node/npm por separado: sin `java` no hay `keytool`
   (firmas/huellas), ni `sdkmanager` (NDK), ni Gradle
7. Avisa si `ANDROID_HOME` no está configurado
8. Menú contextual: **no** se instala por defecto (el de Windows es registro).
   Opt-in con `./install.sh --with-context-menu`

## Testing

No hay suite de tests automatizada. La verificación es compilación exitosa
(`cargo build`) más pruebas manuales en la máquina de destino.
