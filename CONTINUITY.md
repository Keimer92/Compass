# Code Runner — Continuity Document

Este documento resume TODO lo construido hasta ahora para que cualquier IA pueda continuar el proyecto sin perder contexto.

---

## Objetivo

CLI en Rust (`code-runner`) que reemplaza un script `dev.ps1` de PowerShell. Se instala globalmente con `cargo install --path . --force` y se ejecuta desde cualquier directorio de un proyecto Tauri. Provee un menu interactivo con cards (ratatui) para tareas de desarrollo, build, limpieza, firma Android y compilacion del motor de WhatsApp.

## Instalacion

```powershell
cd D:\Mis Proyectos\Tauri\Developing Runner
cargo install --path . --force
```

Ejecutar: `code-runner` (menu interactivo) o `code-runner --task <nombre>`.

### Scripts auxiliares

- `Run.bat` — lanzador con doble-click (eleva permisos via PowerShell)
- `install.bat` — instalacion rapida en maquina nueva
- `REINSTALL.md` — pasos manuales de reinstalacion

## Arquitectura

```
src/
├── main.rs              # Entry point, CLI args con clap
├── menu.rs              # TUI con ratatui (cards, mouse, teclado)
├── tasks/
│   ├── mod.rs           # Declaracion de modulos
│   ├── android.rs       # Dev, build arm64, build all, debug, configure signing
│   ├── cleanup.rs       # cache y project cleanup
│   ├── git.rs           # git init + primer commit a main
│   ├── gitignore.rs     # Crear .gitignore para Rust/Tauri
│   ├── keystore.rs      # Gestionar llaves (keystore)
│   ├── signer.rs        # Flujo completo de firma Android
│   ├── terminal.rs      # Configurar autocompletado de terminal
│   ├── whatsapp.rs      # Compilar motor de WhatsApp
│   └── windows.rs       # Dev y build para Windows
└── utils/
    ├── mod.rs
    ├── cmd.rs           # Ejecucion de comandos, clear_screen, run_npm_install
    └── context.rs       # Deteccion del proyecto (Tauri root, ANDROID_HOME, NDK, etc.)
```

## Menu de opciones

El menu se muestra como cards organizadas en 3 grupos:

| Grupo     | Color    | Items |
|-----------|----------|-------|
| DESARROLLO | Cyan     | Windows Dev, Android Dev, Windows Build, Android arm64, Android All |
| MANTENIMIENTO | Yellow | Clean Cache, Debug Android, Configure Signing, Clean Project |
| CONFIGURACION | Magenta | .gitignore, Git Init, Keystore, WhatsApp, Terminal |

Navegacion: `flechas + Enter`, click con mouse, `Esc` para salir.

Tambien se puede ejecutar directamente:
```
code-runner --task git-init
code-runner --task whatsapp
code-runner --task sign
```

## Decisiones clave

### TUI con ratatui
- Se reemplazo `inquire::Select` por `ratatui + crossterm` para soporte de cards con mouse.
- Mouse habilitado via `crossterm::EnableMouseCapture`.
- Se entra a alternate screen durante el menu, se restaura antes de ejecutar tasks.

### Windows y npm
- `npm` en Windows es `npm.cmd`. Debe ejecutarse con `cmd /c npm.cmd` o mediante `cmd::run_npm_install()`.
- Para tareas que necesitan terminal separada (ej. `npm run tauri dev`), se usa `CREATE_NEW_CONSOLE`.

### Motor de WhatsApp
El motor vive en `src-tauri/whatsapp-engine/` y se compila con un bundler.

**Proyectos soportados:**
- **School Tracker Pro ELA** (`D:\Mis Proyectos\Tauri\School Tracker Pro ELA`) — usa `@vercel/ncc` en `bundle.mjs`. Compila directo, **no necesita** `external`.
- **GymPro** (`D:\Mis Proyectos\Tauri\GymPro`) — migrado de `esbuild` a `@vercel/ncc` (Jun 2026). Tambien usa ncc.

Si en el futuro otro proyecto usa `esbuild` directo, hay que agregar `external` para los paquetes opcionales de baileys:
```js
external: ['jimp', 'sharp', 'link-preview-js', 'qrcode-terminal']
```

### Firma Android (signer)
- El flujo completo de firma (buscar keytool, generar keystore, escribir `keystore.properties`, parchear `build.gradle.kts`) esta inline en `tasks/signer.rs`.
- No depende de un crate externo `tauri-android-signer`.

## Variables de entorno detectadas
- `ANDROID_HOME` — ruta al SDK de Android
- `ADB` (derivado de ANDROID_HOME)
- `NDK` (derivado de ANDROID_HOME)

## Proyectos en los que se ha probado

| Proyecto | Ruta | WhatsApp engine compilation |
|----------|------|---------------------------|
| School Tracker Pro ELA | `D:\Mis Proyectos\Tauri\School Tracker Pro ELA` | (ncc) |
| GymPro | `D:\Mis Proyectos\Tauri\GymPro` | (ncc, migrado desde esbuild) |

## Estado actual

-  Todo funcional y probado
-  Compilacion e instalacion global verificada
-  Ultimo build release instalado: `%USERPROFILE%\.cargo\bin\code-runner.exe`

## Archivos relevantes

| Archivo | Proposito |
|---------|-----------|
| `src/main.rs` | Entry point, argumentos CLI |
| `src/menu.rs` | Menu interactivo con cards (ratatui) |
| `src/utils/context.rs` | Deteccion del proyecto Tauri |
| `src/utils/cmd.rs` | Helpers de comandos (npm, clear) |
| `src/tasks/signer.rs` | Firma Android inline |
| `src/tasks/whatsapp.rs` | Compilacion del motor WhatsApp |
| `src/tasks/git.rs` | Git init + primer commit |
| `src/tasks/gitignore.rs` | Crear .gitignore |
| `Run.bat` | Lanzador con doble-click |
| `install.bat` | Instalacion rapida |
| `REINSTALL.md` | Instrucciones de reinstalacion |
| `CONTINUITY.md` | Este documento |
