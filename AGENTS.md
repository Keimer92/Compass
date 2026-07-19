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
  ├─ MenuItem (15+ variants: WindowsDev, AndroidDev, ..., ContextMenu, Exit)
  ├─ ratatui TUI con cards en grilla 3-columnas, 3 secciones (DEV/MNT/CFG)
  ├─ Filtro por tipo de proyecto (Rust oculta Android/Signing/Keystore)
  ├─ handle_event(): teclado + mouse (cálculo matemático de grid)
  └─ run(): loop: draw → handle → restore → execute → re-enter

utils/
  ├─ context.rs — detect(), ProjectType, ProjectContext, whatsapp_path()
  └─ cmd.rs — run*, spawn_new_window, run_npm_install, clear_screen, remove_dir

tasks/
  ├─ windows.rs — dev(), build()                         [Tauri only]
  ├─ android.rs — dev(), build_arm64(), build_all(), debug(), configure_signing()
  ├─ flutter.rs — run_windows(), run_android(), build_*(), clean()
  ├─ kotlin.rs  — run_windows() [con hot reload], run_android(), build_*(), clean()
  ├─ rust.rs    — run(), build(), clean()                [Rust only]
  ├─ cleanup.rs — clean_cache() [multi-type], clean_project() [Tauri only]
  ├─ git.rs     — init()
  ├─ gitignore.rs — create() [templates por tipo, incluye Rust]
  ├─ keystore.rs — manage() [Tauri: menú backup/restore]
  ├─ signer.rs  — create_or_import(), patch_build_gradle()
  ├─ terminal.rs — configure() [PSReadLine profile]
  ├─ context_menu.rs — install/uninstall/toggle para CodeRunner + OpenCode
  └─ whatsapp.rs — build()
```

## Task Routing

### Por tipo de proyecto

| Menu Item | Tauri | Flutter | Kotlin | Rust |
|-----------|-------|---------|--------|------|
| WindowsDev | `windows::dev` | `flutter::run_windows` | `kotlin::run_windows` | `rust::run` |
| AndroidDev | `android::dev` | `flutter::run_android` | `kotlin::run_android` | oculto |
| WindowsBuild | `windows::build` | `flutter::build_windows` | `kotlin::build_windows` | `rust::build` |
| AndroidBuildArm64 | `android::build_arm64` | `flutter::build_apk` | `kotlin::build_apk` | oculto |
| AndroidBuildAll | `android::build_all` | `flutter::build_aab` | `kotlin::build_aab` | oculto |
| CleanCache | `cleanup::clean_cache` | `cleanup::clean_cache` | `cleanup::clean_cache` | `cleanup::clean_cache` |
| DebugAndroid | `android::debug` | mensaje info | mensaje info | oculto |
| LaunchEmulator | `android::launch` | `android::launch` | `android::launch` | oculto |
| ConfigureSigning | `android::configure_signing` | `android::configure_signing` | `android::configure_signing` | oculto |
| CleanProject | `cleanup::clean_project` | `flutter::clean` | `kotlin::clean` | `rust::clean` |
| CreateGitignore | `gitignore::create` | `gitignore::create` | `gitignore::create` | `gitignore::create` |
| InitGit | `git::init` | `git::init` | `git::init` | `git::init` |
| ManageKeystore | `keystore::manage` | `android::configure_signing` | `android::configure_signing` | oculto |
| BuildWhatsApp | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` |
| LaunchEmulator | `android::launch` | `android::launch` | `android::launch` | oculto |
| ConfigureTerminal | `terminal::configure` | `terminal::configure` | `terminal::configure` | `terminal::configure` |
| InstallContextMenu | `context_menu::toggle` | `context_menu::toggle` | `context_menu::toggle` | `context_menu::toggle` |
| Exit | `process::exit(0)` | `process::exit(0)` | `process::exit(0)` | `process::exit(0)` |

### Tareas globales (sin projecto)

| Task | Ruta |
|------|------|
| `context-menu-install` | `context_menu::install()` |
| `context-menu-uninstall` | `context_menu::uninstall()` |
| `opencode-install` | `context_menu::install_opencode()` |
| `opencode-uninstall` | `context_menu::uninstall_opencode()` |

## Path Conventions

### Android signing

| Type | Android dir | Props file |
|------|------------|------------|
| Tauri | `src-tauri/gen/android/app/` | `keystore.properties` |
| Flutter | `android/app/` | `key.properties` |
| Kotlin | `composeApp/` | `keystore.properties` |
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

Dos entradas en el menú contextual del Explorador de Windows:

| Entrada | Comando | Clave Registro |
|---------|---------|----------------|
| **Open with Code Runner** | `"<exe_path>" "%1"` | `HKCR\Directory\shell\zz_CodeRunner` |
| **Open with OpenCode** | `cmd.exe /c start "OpenCode" /D "%1" cmd.exe /c "opencode"` | `HKCR\Directory\shell\zz_OpenCode` |

- Ambas entradas usan prefijo `zz_` para aparecer al final del menú (orden alfabético).
- Se registran para **carpeta** (`Directory`) y **fondo** (`Directory\Background`).
- Icono: busca `icon.ico` (CodeRunner) o `opencode.ico` (OpenCode) en `%USERPROFILE%\.config\code-runner\`, si no existe usa `Default.ico` del mismo directorio. Como último fallback, queda sin icono.

## Key Patterns

### New window execution
Tareas largas (dev servers, builds) se lanzan en **nueva ventana PowerShell** con `CREATE_NEW_CONSOLE` vía `cmd::spawn_new_window()`. La TUI retorna inmediatamente.

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

### PowerShell registry (PS v5.1)
- Usar `-Type` en lugar de `-PropertyType` (este último solo funciona en PowerShell Core).
- `Test-Path -PathType Leaf` no funciona en claves de registro; se omite.

### Directory removal (Windows)
`cmd::remove_dir()` usa `powershell Remove-Item -Recurse -Force` para rutas largas que `std::fs::remove_dir_all` no puede manejar.

### TUI Menu — Ventana separada
Todas las tareas del menú TUI se lanzan en una **nueva ventana PowerShell** ejecutando `code-runner --task <task> --path "<proyecto>"`. El menú principal permanece abierto. La ventana secundaria se cierra al presionar Enter.

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
4. **Siempre** ejecuta `code-runner --task context-menu-install` (instala ambos menús)
5. Si `cargo install` falla, muestra advertencia pero no detiene la copia de iconos ni la instalación de menús

## Testing

No hay suite de tests automatizada. La verificación es compilación exitosa (`cargo build`).
