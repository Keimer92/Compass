# developing-runner — Continuity Document

Este documento resume TODO lo construido hasta ahora para que cualquier IA pueda continuar el proyecto sin perder contexto.

---

## Objetivo

CLI en Rust (`developing-runner`) que reemplaza un script `dev.ps1` de PowerShell. Se instala globalmente con `cargo install --path . --force` y se ejecuta desde cualquier directorio de un proyecto Tauri. Provee un menú interactivo para tareas de desarrollo, build, limpieza, firma Android y compilación del motor de WhatsApp.

## Instalación

```powershell
cd D:\Mis Proyectos\Tauri\Developing Runner
cargo install --path . --force
```

Ejecutar: `developing-runner` (menú interactivo) o `developing-runner --task <nombre>`.

### Scripts auxiliares

- `Run.bat` — lanzador con doble-click (eleva permisos vía PowerShell)
- `install.bat` — instalación rápida en máquina nueva
- `REINSTALL.md` — pasos manuales de reinstalación

## Arquitectura

```
src/
├── main.rs              # Entry point, CLI args con clap
├── menu.rs              # Loop interactivo con inquire::Select
├── tasks/
│   ├── mod.rs           # Declaración de módulos
│   ├── android.rs       # Dev, build arm64, build all, debug, configure signing
│   ├── cleanup.rs       # cache y project cleanup
│   ├── git.rs           # git init + primer commit a main
│   ├── gitignore.rs     # Crear .gitignore para Rust/Tauri
│   ├── keystore.rs      # Gestionar llaves (keystore)
│   ├── signer.rs        # Flujo completo de firma Android (keytool, keystore.properties, build.gradle.kts)
│   ├── terminal.rs      # Configurar autocompletado de terminal
│   ├── whatsapp.rs      # Compilar motor de WhatsApp
│   └── windows.rs       # Dev y build para Windows
└── utils/
    ├── mod.rs
    ├── cmd.rs           # Ejecución de comandos, clear_screen, run_npm_install
    └── context.rs       # Detección del proyecto (Tauri root, ANDROID_HOME, NDK, etc.)
```

## Menú de opciones

| # | Icono | Sección | Acción |
|---|-------|---------|--------|
| 1 | 🖥 | DEV | Windows (Desarrollo) |
| 2 | 📱 | DEV | Android (Desarrollo) |
| 3 | 📦 | DEV | Windows Build (Release) |
| 4 | 🚀 | DEV | Android Build (Release - arm64) |
| 5 | 🚀 | DEV | Android Build (Release - Todas) |
| 6 | 🧹 | MNT | Limpiar cache y dependencias |
| 7 | 🔍 | MNT | Depurar Android (Chrome Inspect) |
| 8 | 🔑 | MNT | Configurar firmas Android (Release) |
| 9 | 🧼 | MNT | Limpiar proyecto para portabilidad |
| 10 | 📄 | CFG | Crear .gitignore |
| 11 | 🔧 | CFG | Inicializar Git + primer commit |
| 12 | 🗝 | CFG | Gestionar llaves (keystore) |
| 13 | 💬 | CFG | Compilar motor de WhatsApp |
| 14 | ⚙ | CFG | Configurar terminal (autocompletado) |
| 15 | ✕ | | Salir |

También se puede ejecutar directamente:
```
developing-runner --task git-init
developing-runner --task whatsapp
developing-runner --task sign
...
```

## Decisiones clave

### Windows y npm
- `npm` en Windows es `npm.cmd`. Debe ejecutarse con `cmd /c npm.cmd` o mediante `cmd::run_npm_install()`.
- Para tareas que necesitan terminal separada (ej. `npm run tauri dev`), se usa `CREATE_NEW_CONSOLE`.

### Motor de WhatsApp
El motor vive en `src-tauri/whatsapp-engine/` y se compila con un bundler.

**Proyectos soportados:**
- **School Tracker Pro ELA** (`D:\Mis Proyectos\Tauri\School Tracker Pro ELA`) — usa `@vercel/ncc` en `bundle.mjs`. Compila directo, **no necesita** `external`.
- **GymPro** (`D:\Mis Proyectos\Tauri\GymPro`) — migrado de `esbuild` a `@vercel/ncc` (Jun 2026). También usa ncc.

Si en el futuro otro proyecto usa `esbuild` directo, hay que agregar `external` para los paquetes opcionales de baileys:
```js
external: ['jimp', 'sharp', 'link-preview-js', 'qrcode-terminal']
```

### Firma Android (signer)
- El flujo completo de firma (buscar keytool, generar keystore, escribir `keystore.properties`, parchear `build.gradle.kts`) está inline en `tasks/signer.rs`.
- No depende de un crate externo `tauri-android-signer`.

### UI del menú
- Se usa `inquire` (no `ratatui`, no BubbleTea/Go) por simplicidad y navegación por teclado.
- La pantalla se limpia con `cmd::clear_screen()` antes de cada render y antes de cada tarea.

## Variables de entorno detectadas
- `ANDROID_HOME` — ruta al SDK de Android
- `ADB` (derivado de ANDROID_HOME)
- `NDK` (derivado de ANDROID_HOME)

## Proyectos en los que se ha probado

| Proyecto | Ruta | WhatsApp engine compilation |
|----------|------|---------------------------|
| School Tracker Pro ELA | `D:\Mis Proyectos\Tauri\School Tracker Pro ELA` | ✅ (ncc) |
| GymPro | `D:\Mis Proyectos\Tauri\GymPro` | ✅ (ncc, migrado desde esbuild) |

## Estado actual

- ✅ Todo funcional y probado
- ✅ Compilación e instalación global verificada
- ⚠️ Advertencias conocidas (inocuas): campos `has_tauri` y método `assert_node_modules` no usados
- 📦 Último build release instalado: `%USERPROFILE%\.cargo\bin\developing-runner.exe`

## Archivos relevantes

| Archivo | Propósito |
|---------|-----------|
| `src/main.rs` | Entry point, argumentos CLI |
| `src/menu.rs` | Menú interactivo |
| `src/utils/context.rs` | Detección del proyecto Tauri |
| `src/utils/cmd.rs` | Helpers de comandos (npm, clear) |
| `src/tasks/signer.rs` | Firma Android inline |
| `src/tasks/whatsapp.rs` | Compilación del motor WhatsApp |
| `src/tasks/git.rs` | Git init + primer commit |
| `src/tasks/gitignore.rs` | Crear .gitignore |
| `Run.bat` | Lanzador con doble-click |
| `install.bat` | Instalación rápida |
| `REINSTALL.md` | Instrucciones de reinstalación |
| `CONTINUITY.md` | Este documento |
