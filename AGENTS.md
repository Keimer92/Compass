# Code Runner — Developer Guide

## Overview

`code-runner` es una herramienta CLI/TUI multiplataforma que automatiza tareas de desarrollo, compilación y configuración para proyectos **Tauri (Rust)**, **Flutter (Dart)** y **Kotlin Multiplatform (Compose)**. Detecta automáticamente el tipo de proyecto y adapta comandos, rutas y comportamientos.

## Project Types

| Tipo | Detección | Emoji |
|------|-----------|-------|
| `Tauri` | `tauri.conf.json` o `package.json` | 🦀 |
| `Flutter` | `pubspec.yaml` | 🎯 |
| `Kotlin` | `gradlew.bat` + `settings.gradle.kts` | 🔷 |

El orden de detección es el de la tabla (arriba primero). Si un proyecto coincide con `package.json` pero no con `tauri.conf.json`, se clasifica como Tauri (para proyectos JS/Node).

## ProjectContext

Estructura inyectada en todas las funciones de tarea:

```
ProjectContext
  script_dir: PathBuf           # Raíz del proyecto detectado
  project_type: ProjectType     # Tauri | Flutter | Kotlin
  android_home: Option<PathBuf>
  ndk_home: Option<PathBuf>
  adb_path: Option<PathBuf>
  sdk_manager: Option<PathBuf>
  has_node_modules: bool
  has_android_project: bool
  has_whatsapp_engine: bool     # engine_dir existe
```

## Module Map

```
main.rs
  └─ Cli (clap) → --task, --path
      ├─ sin args → menu::run()
      └─ con args → run_task_direct() dispatches by task name

menu.rs
  ├─ MenuItem (15 variants: WindowsDev, AndroidDev, ..., Exit)
  ├─ ratatui TUI con cards en grilla 3-columnas, 3 secciones (DEV/MNT/CFG)
  ├─ handle_event(): teclado + mouse (cálculo matemático de grid)
  └─ run(): loop: draw → handle → restore → execute → re-enter

utils/
  ├─ context.rs — detect(), ProjectType, ProjectContext
  └─ cmd.rs — run*, spawn_new_window, run_npm_install, clear_screen

tasks/
  ├─ windows.rs — dev(), build()           [Tauri only]
  ├─ android.rs — dev(), build_arm64(), build_all(), debug(), configure_signing()
  ├─ flutter.rs — run_windows(), run_android(), build_*(), clean()
  ├─ kotlin.rs  — run_windows() [con hot reload], run_android(), build_*(), clean()
  ├─ cleanup.rs — clean_cache() [multi-type], clean_project() [Tauri only]
  ├─ git.rs     — init()
  ├─ gitignore.rs — create() [templates por tipo]
  ├─ keystore.rs — manage() [Tauri: menú backup/restore]
  ├─ signer.rs  — create_or_import(), patch_build_gradle()
  ├─ terminal.rs — configure() [PSReadLine profile]
  └─ whatsapp.rs — build()
```

## Task Routing

| Menu Item | Tauri | Flutter | Kotlin |
|-----------|-------|---------|--------|
| WindowsDev | `windows::dev` | `flutter::run_windows` | `kotlin::run_windows` |
| AndroidDev | `android::dev` | `flutter::run_android` | `kotlin::run_android` |
| WindowsBuild | `windows::build` | `flutter::build_windows` | `kotlin::build_windows` |
| AndroidBuildArm64 | `android::build_arm64` | `flutter::build_apk` | `kotlin::build_apk` |
| AndroidBuildAll | `android::build_all` | `flutter::build_aab` | `kotlin::build_aab` |
| CleanCache | `cleanup::clean_cache` | `cleanup::clean_cache` | `cleanup::clean_cache` |
| DebugAndroid | `android::debug` | mensaje info | mensaje info |
| ConfigureSigning | `android::configure_signing` | `android::configure_signing` | `android::configure_signing` |
| CleanProject | `cleanup::clean_project` | `flutter::clean` | `kotlin::clean` |
| CreateGitignore | `gitignore::create` | `gitignore::create` | `gitignore::create` |
| InitGit | `git::init` | `git::init` | `git::init` |
| ManageKeystore | `keystore::manage` | `android::configure_signing` | `android::configure_signing` |
| BuildWhatsApp | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` |
| ConfigureTerminal | `terminal::configure` | `terminal::configure` | `terminal::configure` |
| Exit | `process::exit(0)` | `process::exit(0)` | `process::exit(0)` |

## Path Conventions

### Android signing

| Type | Android dir | Props file |
|------|------------|------------|
| Tauri | `src-tauri/gen/android/app/` | `keystore.properties` |
| Flutter | `android/app/` | `key.properties` |
| Kotlin | `composeApp/` | `keystore.properties` |

### WhatsApp engine

| Type | Engine dir | Bundle output |
|------|-----------|--------------|
| Tauri | `src-tauri/whatsapp-engine/` | `src-tauri/resources/whatsapp-engine-bundle/` |
| Flutter | `whatsapp_engine/` | `resources/whatsapp-engine-bundle/` |
| Kotlin | `whatsapp_engine/` | `composeApp/resources/whatsapp-engine-bundle/` |

### Android project detection

| Type | Path checked |
|------|-------------|
| Tauri | `src-tauri/gen/android/gradlew.bat` |
| Flutter | `android/gradlew.bat` |
| Kotlin | `gradlew.bat` (raíz) |

## Key Patterns

### New window execution
Tareas largas (dev servers, builds) se lanzan en **nueva ventana PowerShell** con `CREATE_NEW_CONSOLE` vía `cmd::spawn_new_window()`. La TUI retorna inmediatamente.

### npm en Windows
`cmd::run_npm_install()` usa `cmd /c npm.cmd install` — necesario porque `npm` en Windows es realmente `npm.cmd`.

### Gradle en Windows (KMP)
Los comandos de Gradle se ejecutan como `cmd /c gradlew.bat <args>` porque PowerShell no puede ejecutar `.bat` directamente.

### Hot reload detection (KMP)
`kotlin::run_windows()` verifica si `composeApp/build.gradle.kts` contiene `hot-reload`, `hotReload` o `hot_run`. Si existe, usa `:composeApp:hotRunDesktop --auto`.

### Android signing
`signer::patch_build_gradle()` soporta tanto `build.gradle.kts` (Kotlin Script) como `build.gradle` (Groovy), inyectando:
1. Properties loader (variables desde `keystore.properties`/`key.properties`)
2. `signingConfigs { release { ... } }`
3. `signingConfig` en build type release

### CLI directo (sin TUI)
```bash
code-runner --task windows-dev
code-runner --path /ruta/proyecto --task sign
code-runner  # abre menú interactivo
```

## .gitignore Templates

Cada tipo de proyecto tiene su propio `.gitignore`:

- **Tauri**: ignorar Rust target, gen/android, node_modules, dist, Vite cache
- **Flutter**: ignorar build, .dart_tool, Android builds, ios symlinks
- **Kotlin**: ignorar build, .gradle, .kotlin, local.properties, APK/AAB artefactos

Todos preservan: fuente, config, wrapper/scripts, firmas, motor WhatsApp.

## Testing

No hay suite de tests automatizada. La verificación es compilación exitosa (`cargo build`).
