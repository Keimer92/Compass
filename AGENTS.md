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
- **Un wrapper `vibe-<slug>.sh` por herramienta** en `$XDG_DATA_HOME/code-runner/vibe/`, en vez de incrustar `sh -c "cd %f && …"`. El `cd` y el quoting viven en el archivo (se escapan una vez); el preset sólo invoca el wrapper. ⚠️ El cuerpo del wrapper lo emite `cmd::terminal_launch_shell()`, y su rama de kitty tiene un **doble parseo** documentado en "Bug conocido: doble parseo en la rama kitty-tab".
- **Limitación: sin submenús anidados.** `ContextActionPreset` sólo tiene `name`, `confirm`, `selection`, `steps` — no hay campo de agrupación. Las entradas quedan **planas**, no en cascada como en Windows. El prefijo `"Vibe Coding: "` las agrupa visualmente y es el criterio que usa `uninstall()` para borrarlas sin tocar las del usuario.
- `install()` hace **merge**: preserva los presets previos del usuario, y sólo reemplaza los que empiezan con el prefijo. Respalda el archivo previo a `context_actions.ron.code-runner-bak` antes de escribir.
- Requiere cerrar y reabrir COSMIC Files. Sin `cosmic-files` en el PATH, `install()` falla con mensaje explícito.
- `VIBE_TOOLS` está duplicado (el de `context_menu.rs` usa `key` de registro, que aquí no aplica). Si se unifica, debe subir a un módulo común. ⚠️ **Hay que actualizar los dos arrays a la vez**: añadir una herramienta sólo al de Windows deja el menú de COSMIC Files sin ella, sin error ni aviso.

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

### run-sh: ejecutar `.sh` con doble clic

Utilidad **del sistema**, no de code-runner, pero `install.sh` la instala siempre.

| Pieza | Ruta |
|-------|------|
| Script | `~/.local/bin/run-sh` |
| Entry | `~/.local/share/applications/run-sh.desktop` (`NoDisplay=true`) |
| Default | `application/x-shellscript` en `~/.config/mimeapps.list` |

- **`Exec` con ruta absoluta**: cosmic-files no tiene `~/.local/bin` en el PATH,
  así que `Exec=run-sh %f` no se resolvería.
- **`Terminal=false` a propósito**: es `run-sh` quien abre la terminal, y así
  controla intérprete, código de salida y ventana abierta al final.
- **El intérprete sale del shebang**, no de asumir `sh`: un `#!bash` ejecutado
  con dash falla con errores de sintaxis que no dicen nada.
- **Cambia el default de todos los `.sh`**: antes se abrían en Cosmic Edit. Éste
  sigue registrado ("Abrir con"), y para revertir:
  `xdg-mime default com.system76.CosmicEdit.desktop application/x-shellscript`.
- `xdg-mime default` preserva el resto de entradas de `mimeapps.list`; si no
  está, `install.sh` edita la sección `[Default Applications]` a mano.

Dos trampas del propio script, ya corregidas y documentadas en él: `set --` para
parsear el shebang pisaba los argumentos del script, y `$'\n'` no expande en
todos los `sh` (la cola se arma con `printf '%s\n'`).

Aviso al verificar que el script recibe un tty: comprobar `[ -t 1 ]` **antes** de
cualquier redirección. Si el script hace `> fichero` y luego comprueba `-t 1`, da
falso por definición y parece un fallo del runner.

### Atajos de terminal (`assets/scripts/`, instalados en `~/.local/bin`)

| Script | Qué hace |
|--------|----------|
| `crh` | TUI en `$PWD`. Atajo de `code-runner-here`, que se retiró. |
| `code-runner-open [RUTA]` | TUI en `RUTA`, o en la carpeta de un fichero. Acepta `%f`. |

Se instalan con `sed s/@EXE@/ruta/g`, igual que el `.desktop`. `install.sh`
retira los nombres antiguos para no dejar dos comandos haciendo lo mismo.

Existen por una limitación de cosmic-files: las `context_actions` **sólo se
muestran con algo seleccionado**, así que el clic en el fondo de la carpeta no
puede lanzar nada. El flujo equivalente es "Open in Terminal" → `crh`.

### Estados visuales de una card (`menu.rs::create_card`)

Tres estados, deliberadamente distintos entre sí:

| Estado | Borde | Fondo |
|--------|-------|-------|
| reposo | `DarkGray` | ninguno |
| hover | `White` + BOLD | `hover_tint()` = `Rgb(30,30,36)` neutro |
| seleccionado | color de sección + BOLD | `glass_tint(sección)` teñido |

⚠️ Antes hover y selección compartían `glass_tint` y sólo se diferenciaban por el
`BOLD` del borde: el hover pasaba desapercibido. Cubierto por
`menu::tests::hover_is_visually_distinct_from_selected`; si el tint vuelve a
compartirse, el test falla.

Si una card está seleccionada **y** hovereada, manda la selección: es la que
Enter ejecutaría.

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

### 🐛 Bug conocido: doble parseo en la rama kitty-tab

**Sin arreglar a propósito.** Está en `terminal_launch_shell` (`src/utils/cmd.rs`), rama `spec.bin == "kitty"`, que es lo que se hornea en los wrappers `vibe-*.sh` de `context_menu_cosmic.rs`. **Sin verificar en Linux**: no hay forma de probarlo sin un `kitty` real en Pop!_OS.

Hoy emite:

```sh
exec script -qec "kitty @ --to unix:$socket launch --type=tab --title 'X' sh -lc \"$CR_SCRIPT\"" /dev/null
```

El cuerpo del script **se parsea dos veces**. La primera da un argv con `sh -lc "cd "$target" || exit 1; …"`; el shell intermedio que invoca `script -c` re-parsea esa cadena, y ahí:

- `$target` está **sin definir** en ese shell y **sin comillas** (porque las comillas del nivel anterior ya se consumieron), así que se expande a vacío.
- La expansión sin comillas **parte por IFS**: `sh -lc` recibe el primer trozo como `-c` y el resto como argumentos sueltos, o sea que el comando se trunca.

Síntoma esperado: la pestaña de kitty se abre pero `cd "$target"` falla o el script se queda en `target: not found`. **La rama de ventana nueva** (`exec kitty --title 'X' -e sh -lc "$CR_SCRIPT"`) sí es correcta: kitty hace `exec` directo de `sh -lc <script>` sin re-parsear, porque `"$CR_SCRIPT"` llega como un único elemento de argv.

**Por qué se llegó aquí**: la versión anterior embebía el script directamente en la cadena de `script -qec`, y `script` abortaba con *"unexpected number of arguments"* al encontrar el `"$target"` o el `read -r` de dentro. Pasarlo por variable de entorno arregla el quoting *de esa* capa, pero no el del shell intermedio.

**Arreglo propuesto** (no aplicado): que sea el shell intermedio el que cite, y que el script llegue a kitty como tres argv distintos en vez de re-parseado. El `export` es lo que hace que el shell intermedio herede las variables:

```sh
CR_SCRIPT=$(cat <<'CR_EOF'
{script}
CR_EOF
)
export CR_SCRIPT CR_SOCKET CR_TITLE
if [ -n "$CR_SOCKET" ]; then
    exec script -qec 'kitty @ --to "unix:$CR_SOCKET" launch --type=tab --title "$CR_TITLE" sh -lc "$CR_SCRIPT"' /dev/null
fi
exec kitty --title "$CR_TITLE" -e sh -lc "$CR_SCRIPT"
```

Lo de `script -qec '…'` con comillas simples es lo clave: el wrapper no expande nada y el shell intermedio recibe las tres variables ya como argumentos entrecomillados.

**Cómo verificarlo** (Pop!_OS, sin emulador): `ss -xlH | grep kitty-code-runner` debe listar `@kitty-code-runner-<pid>`, que es el requisito para que se pruebe la rama de pestaña en vez de la de ventana. Sin instancia de kitty abierta, la rama con el bug no se ejecuta y el fallo queda oculto.

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
  ├─ platform.rs — helpers de portabilidad: IS_WINDOWS, exe_name(), gradlew_name(), home_dir(), data_dir(), find_gradlew(), NDK_VERSION, tauri_native_pkg()
  ├─ cmd.rs — run*, spawn_new_window, spawn_new_window_autoexit, spawn_new_window_interactive, run_npm_install, clear_screen, remove_dir, reveal_on_success
  ├─ tauri.rs — command(), ensure(), cli_entry() [CLI sin shim de .bin + reinstalación si falta el binding nativo]
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
  ├─ terminal.rs — configure() [perfil PSReadLine según la versión instalada]
  ├─ context_menu.rs — install/uninstall/toggle para CodeRunner + Vibe Coding (menú cascada con ExtendedSubCommandsKey: OpenCode, Kilo Code, Cline, Antigravity, Kiro CLI, Hermes)
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
| `vibe-setup` | `context_menu::setup()` → `ensure_tools_installed()` (pregunta por las que falten) + `install_vibe_coding()` |
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

### Vibe Coding (menú cascada: 6 agentes)

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
  05KiroCli      → command
  06Hermes       → command
```

⚠️ Las sub-entradas **no** cuelgan de `zz_VibeCoding\shell`: cuelgan de `Directory\ContextMenus\VibeCoding\shell` (ruta compartida con `Directory\Background`), y `zz_VibeCoding` sólo lleva el puntero `ExtendedSubCommandsKey`. Buscar en el sitio equivocado es lo que hacía que `install.bat` reinstallara el menú en cada ejecución.

| # | Sub-entrada | Comando | Descripción |
|---|-------------|---------|-------------|
| 01 | **OpenCode** | `cmd.exe /c start "OpenCode" /D "%V" cmd.exe /c "opencode --auto"` | Abre OpenCode en la carpeta |
| 02 | **Kilo Code** | `cmd.exe /c start "Kilo Code" /D "%V" cmd.exe /c "kilo"` | Abre Kilo Code en la carpeta |
| 03 | **Cline** | `cmd.exe /c start "Cline" /D "%V" cmd.exe /c "cline -i"` | Abre Cline (`-i` evita modo headless) |
| 04 | **Antigravity** | `cmd.exe /c start "Antigravity" /D "%V" cmd.exe /c "agy"` | Abre Antigravity CLI (Google) |
| 05 | **Kiro CLI** | `cmd.exe /c start "Kiro CLI" /D "%V" cmd.exe /c "kiro-cli chat"` | Abre Kiro CLI. El binario es **`kiro-cli`**, no `kiro` |
| 06 | **Hermes** | `cmd.exe /c start "Hermes" /D "%V" cmd.exe /c "hermes"` | Abre Hermes (Nous Research), sin flags: su TUI arranca a pelo |

⚠️ **Kiro CLI no tiene comando de instalación**: se instala con su `.exe` (o `winget install Kiro-CLI`), no desde la terminal. Por eso `install` va **vacío** en su entrada de `VIBE_TOOLS`, y eso significa explícitamente **"detectar e instalar el submenú, pero no preguntar por instalarlo"**. `ensure_tools_installed()` salta el bucle de preguntas cuando `install` está vacío y sólo avisa. Preguntar algo a lo que no se puede responder con un Enter sería un bucle sin salida. Cubierto por `a_empty_install_means_detect_only_never_ask`.

⚠️ Kiro rompe el patrón de rutas del resto: su instalador lo deja en `%LOCALAPPDATA%\Kiro-Cli\kiro-cli.exe`, **plano y sin subcarpeta `bin`**, al contrario que `%LOCALAPPDATA%\agy\bin` y `%LOCALAPPDATA%\hermes\bin`. La sonda de `install.bat` lo tiene en un mapa `$nombres` por eso.

⚠️ **Hermes y Antigravity no se detectan sólo por el PATH.** Sus instaladores nativos dejan el binario en su propia carpeta (`%LOCALAPPDATA%\hermes\bin`, `%LOCALAPPDATA%\agy\bin`) y no siempre lo agregan al PATH del usuario. Por eso `VibeTool::fallback` declara rutas alternativas con prefijo `LOCALAPPDATA/` o `HOME/`, que `fallback_candidates()` expande. **Los npm globales no necesitan nada de esto** (npm sí modifica el PATH), y hay un test que lo comprueba.

### Instalación de las herramientas (`vibe-setup`)

`ensure_tools_installed()` detecta cuáles de las 6 faltan. **Sólo pregunta por las que tienen instalador**: Kiro CLI se avisa pero no se pregunta, porque va con `.exe`. El comando de instalación vive en `VibeTool::install`, verificado en la documentación oficial de cada producto:

| Herramienta | `install` | `install_via` |
|---|---|---|
| OpenCode | `npm install -g @opencode/cli` | `Cmd` |
| Kilo Code | `npm install -g @kilocode/cli` | `Cmd` |
| Cline | `npm install -g cline` | `Cmd` |
| Antigravity | `irm https://antigravity.google/cli/install.ps1 \| iex` | `PowerShell` |
| Kiro CLI | *(ninguno: `.exe`)* | — |
| Hermes | `iex (irm https://hermes-agent.nousresearch.com/install.ps1)` | `PowerShell` |

⚠️ **`install_via` no es cosmético**: en Windows `npm` es `npm.cmd` y su shim `npm.ps1` lo bloquea la ExecutionPolicy por defecto, así que los npm van por `cmd /c`. Y `irm | iex` es sintaxis de PowerShell, que en `cmd` no existe.

⚠️ El default de la pregunta es **no**. `ask_text` imprime el default entre corchetes, así que el prompt no debe llevar su propio `[s/N]`. Con `None` no habría default y Enter devolvería "" (igual de no, pero la pregunta no se responde con la barra espaciadora). `es_si` acepta `s`/`si`/`sí`/`y`/`yes`.

⚠️ **La verificación de "ha instalado" usa `resolve_tool_command()`, no `command_exists()`**: el instalador de Google deja `agy` en `%LOCALAPPDATA%\agy\bin`, fuera del PATH, así que en el proceso actual `Get-Command agy` sigue dando falso y se reportaría como fallo siendo un éxito. Es el mismo fallback que hace que el menú funcione para Antigravity.

`setup()` reinstala el menú al final aunque ya estuviera al día, porque una herramienta recién instalada tiene que aparecer como sub-entrada nueva.

`install.bat` lanza `vibe-setup` cuando el menú falta **o** cuando la sonda detecta herramientas ausentes, así que si instalas Cline a mano y relanzas el instalador aparece sola su sub-entrada.

**Notas técnicas:**
- **Orden del submenú**: Windows ordena las sub-claves de `shell` **alfabéticamente**. Por eso las claves llevan prefijo numérico (`01OpenCode`, `02KiloCode`, ...) para forzar el orden requerido en `VIBE_TOOLS` (src/tasks/context_menu.rs). Agregar una herramienta = agregar una entrada al final de `VIBE_TOOLS` con prefijo incremental. Hay un test que verifica que los prefijos son `01, 02, 03…` sin huecos ni repeticiones y en orden, porque un prefijo duplicado pierde el orden en silencio.
- `%V` = directorio de la carpeta seleccionada (funciona con `ExtendedSubCommandsKey`; `%1` NO funciona).
- La entrada padre usa `MUIVerb` en vez de `(default)` para el texto visible.
- Las sub-entradas están en `Directory\ContextMenus\VibeCoding\shell\` (ruta compartida para `Directory` y `Directory\Background`).
- Se registran para **carpeta** (`Directory`) y **fondo** (`Directory\Background`).
- `remove_vibe_ctx_tree()` borra el árbol `ContextMenus\VibeCoding` completo en cada instalación — evita claves huérfanas de versiones previas que alteraban el orden.
- Se limpian entradas viejas individuales (`zz_OpenCode`, `zz_pCline`, `zz_pKilo`) al instalar.
- **Antigravity CLI**: binario nativo (no npm) instalado por `irm https://antigravity.google/cli/install.ps1 | iex` → `%LOCALAPPDATA%\agy\bin\agy.exe`. No siempre está en PATH; `resolve_tool_command()` hace fallback a esa ruta si `Get-Command agy` falla.
- **Hermes CLI** (Nous Research): instalador nativo `iex (irm https://hermes-agent.nousresearch.com/install.ps1)`. En Windows nativo vive bajo `%LOCALAPPDATA%\hermes\bin` (el README lo dice explícitamente), en Linux el `install.sh` lo deja en `~/.local/bin`. Mismo caso de fallback que Antigravity. `hermes` arranca su TUI sin flags; `hermes-acp` es el servidor ACP para editores y no va al menú.
- **Kiro CLI** (AWS): se instala con su `.exe` (`winget install Kiro-CLI` o el instalador de kiro.dev), **no hay comando de terminal**. El ejecutable queda en `%LOCALAPPDATA%\Kiro-Cli\kiro-cli.exe` — nótese que **no** sigue el patrón `…\<tool>\bin` del resto, por eso la sonda de `install.bat` lo lleva en un mapa. El subcomando para chatear es `kiro-cli chat`; `--tui` fuerza la interfaz nueva y `--legacy-ui` la antigua. `install` va vacío en `VIBE_TOOLS`, que significa "detectar, nunca preguntar por instalarlo".
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

### CLI de Tauri (`utils/tauri.rs`)
Un `node_modules` compartido entre Windows y Linux rompe el CLI de Tauri por partida doble, y las dos cosas se evitan así:

| Síntoma | Causa |
|---------|-------|
| `'tauri' is not recognized…` | `node_modules/.bin/tauri` es el shim del sistema que hizo el install. En POSIX es un fichero sin extensión; en Windows además salen `tauri.cmd`/`tauri.ps1`. `cmd` sólo resuelve por `PATHEXT`. |
| `Cannot find native binding` | `@tauri-apps/cli` delega en una *optional dependency* por plataforma. Si el install fue en el otro sistema, la de aquí no está ([npm/cli#4828]). |

- `tauri::command(ctx, args)` emite `node node_modules/@tauri-apps/cli/tauri.js <args>`, nunca `tauri` a secas. Cae a `npx tauri` si el paquete no está.
- `tauri::ensure(ctx)` llama a `npm install` si falta `node_modules`, el paquete, o el binding de `platform::tauri_native_pkg()`. Se llama **antes** de `command()`.
- **`npm install` reemplaza** el binding del otro sistema, no lo añade: un `node_modules` en ping-pong entre sistemas pide un install por uso. Inherente al modelo de optional deps.

⚠️ **La ruta va relativa y sin comillas**, y no es estética. Los comandos se lanzan como `cmd /c "title X & <cmd> & pause"`; `cmd` no quita sus comillas cuando el contenido tiene caracteres especiales (`&`), así que unas comillas internas rompen el parseo y `node` recibe la ruta con un `"` pegado (`Cannot find module`). Todos los lanzadores ponen el cwd en la raíz del proyecto, así que la ruta relativa resuelve.

Cubierto por `utils::tauri::tests` (los tres casos: fallback a `npx`, invocación por `node`, y detección del binding de otra plataforma).

### Configurar Terminal (perfil PSReadLine)
`terminal::configure()` **actualiza PSReadLine si hace falta y después** escribe `Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1`. El perfil depende de la versión instalada, no es un texto fijo:

Orden del flujo: detectar versión → si no hay predictivas, `Install-Module` → volver a detectar → escribir el perfil que corresponda.

| Versión tras el paso anterior | Perfil | Qué da |
|---------|--------|--------|
| ≥ 2.2.0 | `PROFILE_PREDICTIVE` | Sugerencias inline (`-PredictionSource History`), Tab acepta sugerencia, → acepta palabra |
| < 2.2.0 | `PROFILE_LEGACY` | Tab = `MenuComplete`, flechas = búsqueda en historial, sin campana, ToolTips |
| no instalado | `PROFILE_SIN_MODULO` | Sólo explica cómo instalarlo, sin emitir comandos |

⚠️ **2.2.0 es el corte**: es la primera versión con *predictive intellisense*. Windows PowerShell 5.1 trae la **2.0.0** de serie, que no conoce ni `-PredictionSource` ni las Functions `AcceptSuggestion` / `AcceptNextSuggestionWord`. Emitir el perfil moderno ahí llenaba cada terminal nueva de `NamedParameterNotFound` y `ParameterArgumentValidationError`. La versión se lee con `Get-Module -ListAvailable` en un `powershell -NoProfile`, y se compara como `major*100 + minor`: `2.11.4` (211) es **más nueva** que `2.2.0` (202), cosa que un `>=` sobre el texto daría al revés.

⚠️ **`Get-Module -ListAvailable` se ejecuta SIEMPRE con `powershell` (5.1), nunca con el `pwsh` de pwsh 7**: el `PSModulePath` de los dos no coincide, y una instalación en `CurrentUser` (`Documents\PowerShell\Modules`) puede existir para 5.1 y no aparecer desde pwsh. Diagnosticar con el shell equivocado hace creer que el módulo falta.

### La actualización (`upgrade_psreadline`)
`Install-Module PSReadLine -Scope CurrentUser -Force -AllowClobber -Confirm:$false`, con tres detalles que hacen falta en 5.1:

- **TLS 1.2** en el mismo `-Command`: PSGallery responde *"Could not create SSL/TLS secure channel"* porque 5.1 negocia TLS 1.0. `SecurityProtocol` es de proceso, así que tiene que ir en la misma invocación.
- **`-Force -AllowClobber -Confirm:$false`**: sin ellos pregunta por consola al reemplazar la versión instalada.
- **`cmd::run_timeout`** (stdin a `/dev/null` + techo de 300 s): la primera vez descarga el proveedor NuGet. Con el stdin cerrado, cualquier prompt lee EOF y aborta en vez de colgar la TUI.

Sólo se llama cuando `is_predictive()` es falso, así que en una máquina al día no se toca la red.

⚠️ **`-PredictionSource` va dentro de un `try`/`catch`**. PSReadLine lanza `ArgumentException` si se activa sin consola interactiva con soporte de terminal virtual — o sea, con la salida redirigida (`powershell -Command`, scripts, o cualquier herramienta que lance PowerShell capturando stdout). Como el perfil se carga también en esos casos, sin el `catch` el error reaparece, sólo que cambiado. Verificado: con el `catch`, `powershell -Command 'exit 0'` deja **0 bytes en stderr**.

⚠️ **El perfil se escribe con BOM UTF-8** (`write_utf8_bom`). PS 5.1 lee un `.ps1` sin BOM como ANSI/CP1252, así que cada acento se parte en dos caracteres (`ú` = bytes `C3 BA` → 195 y 186). Hoy son sólo comentarios, pero cualquier ruta con acento en un alias dejaría de funcionar. El BOM forma parte de la comparación de idempotencia: un perfil correcto *sin* BOM tiene que reescribirse.

Antes de sobrescribir se copia el perfil previo a `Microsoft.PowerShell_profile.ps1.code-runner-bak` (mismo convenio que `context_actions.ron.code-runner-bak`). Si el contenido ya coincide, no se toca el archivo.

Cubierto por `tasks::terminal::tests` (8 tests): parseo de versión, decisión de actualizar, elección de perfil, **los nombres del perfil legacy existen todos en la 2.0.0** (lista real sacada con `Get-Command`), ausencia de nombres predictivos, `-PredictionSource` protegido por `try`/`catch`, y BOM (incluido que su presencia forma parte de la comparación).

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

1. **Salta la compilación si no hace falta** (ver "Saltar lo que ya está al día")
2. **Siempre** copia `assets/icon/Icon.ico` a `%USERPROFILE%\.cargo\bin\`
3. **Siempre** copia `assets/icon/opencode.ico` al mismo directorio
4. Instala los menús contextuales (Code Runner y Vibe Coding), cada uno sólo si la sonda dice que falta
5. Si `cargo install` falla, avisa y **no** configura los menús: apuntar el registro a un binario roto deja el sistema incoherente
6. `install.bat --force` rehace todo aunque no haga falta

### Saltar lo que ya está al día

Reinstalar costaba ~30 s en cada ejecución, casi todo en cosas innecesarias. Los tres pasos se deciden con una **sola** invocación de PowerShell, `assets/install-probe.ps1`, que devuelve `<compilar>;<menu CR>;<menu vibe>` con 1 = actuar:

| Antes | Ahora | Motivo |
|---|---|---|
| `cargo install --path . --force` siempre | sólo si algún archivo de `src/`, `Cargo.toml` o `Cargo.lock` es más nuevo que el `.exe` | el `--force` de cargo no es opcional: sin él se niega a reinstalar la misma versión |
| 2 sondas de registro en PowerShell | 1 sola, con las cuatro comprobaciones | PowerShell 5.1 tarda ~2,5 s en arrancar; 3 llamadas son ~7,5 s |
| Menú de Vibe Coding **siempre** reinstallado | sólo si falta | la sonda miraba `zz_VibeCoding\shell\OpenCode\command`, una clave que **no existe**: el padre sólo lleva `ExtendedSubCommandsKey` y las subentradas viven en `Directory\ContextMenus\VibeCoding\shell\` con prefijo numérico (`01OpenCode`) |
| Sin detección de herramientas ausentes | cuarto campo de la sonda | va en la **misma** invocación de PowerShell, así que no cuesta los ~2,5 s de arranque; avisa en minúscula si falta alguna |
| `timeout /t 3` al final | eliminado | 3 s de espera muerta |

Medido: **~30 s → ~7 s** sin nada que cambiar, ~19 s cuando hay que recompilar, ~32 s con `--force`. El grosso del coste era `opencode-install`, que tardaba **18 s** sólo en detectar las herramientas (un `Get-Command` por herramienta, o sea un arranque de PowerShell cada una). La sonda de `install.bat` hace todas las comprobaciones en **una** invocación, así que añadir herramientas a `VIBE_TOOLS` no encarece el instalador.

⚠️ La sonda vive en un `.ps1` y no en un one-liner dentro de `for /f` porque las tuberías (`|`) son caracteres especiales de `cmd` y rompen el bucle. Y ⚠️ `1` significa **actuar**: leerlo al revés hace reinstallar siempre (que es lo que pasó en la primera versión de este cambio).

⚠️ `install.bat` usa `setlocal EnableDelayedExpansion` y `!errorlevel!` dentro de los bloques: con `%errorlevel%` se evalúa al parsear el `if`, antes de que `cargo` corra, y siempre da 0.

## install.sh

Script de instalación para Linux (Pop!_OS / Ubuntu). Diferencias con `install.bat`:

1. Hace `cd` a su propio directorio (el check de `Cargo.toml` miraba el CWD de
   quien lo invocaba, y fallaba con `./ruta/al/proyecto/install.sh`)
2. Verifica `command -v cargo` en vez de `where cargo`
3. Cierra instancias con `pkill -x code-runner` en vez de `taskkill`
4. Compila con `cargo install --path . --force`, **salvo** que ninguna fuente sea
   más nueva que el binario instalado (`find … -newer`). `install.sh --force`
   fuerza la recompilación. La lógica va en `tools/test-skip.sh`, que se puede
   ejecutar para comprobar los dos casos sin instalar nada
5. Instala la **entrada del launcher** (`.desktop`) e iconos PNG en hicolor
6. Comprueba SDK, **JDK** y node/npm por separado: sin `java` no hay `keytool`
   (firmas/huellas), ni `sdkmanager` (NDK), ni Gradle
7. Avisa si `ANDROID_HOME` no está configurado
8. Menú contextual: **no** se instala por defecto (el de Windows es registro).
   Opt-in con `./install.sh --with-context-menu`

## Testing

`cargo test` cubre el renderizado de las cards del menú (ver "Estados visuales
de una card"). El resto —detección de proyectos, `reveal_on_success`, wrappers
del menú contextual, generación del `.desktop`— se verifica con compilación
(`cargo build`) y pruebas manuales en la máquina de destino.
