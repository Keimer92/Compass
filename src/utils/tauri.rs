//! Localización, invocación y reparación automática del CLI de Tauri.
//!
//! Existe por una razón concreta: los `node_modules` de Tauri incluyen shims y
//! bindings nativos de la plataforma, por lo que un árbol instalado en otro
//! sistema operativo puede no funcionar en Windows:
//!
//! 1. **`node_modules/.bin/tauri` no tiene shim de Windows.** npm genera el shim
//!    del sistema que hizo el install: en Windows existen `tauri.cmd` y `tauri.ps1`. `cmd` resuelve ejecutables
//!    sólo por `PATHEXT` (`.COM/.EXE/.BAT/.CMD/...`), así que con un
//!    `node_modules` traído de Linux, `tauri dev` muere con
//!    *'tauri' is not recognized as an internal or external command*.
//!
//! 2. **Falta el binding nativo.** `@tauri-apps/cli` delega en un binario que
//!    llega como optional dependency con nombre por plataforma
//!    (`@tauri-apps/cli-win32-x64-msvc`, por ejemplo).
//!    Si el install se hizo en otro sistema, el binding de Windows no está y
//!    el CLI aborta con *Cannot find native binding* ([npm/cli#4828]).
//!
//! Ambas se resuelven igual, y por eso este módulo centraliza las dos:
//!
//! - Se invoca `node` sobre el entry point del paquete en vez de `tauri` a
//!   secas, así que el shim de `.bin` deja de importar.
//! - [`ensure`] comprueba que estén el paquete y su binding de Windows y,
//!   si falta cualquiera de los dos, reinstalla antes de devolver el control.
//!
//! Nota: `npm install` reemplaza el binding disponible según el sistema donde
//! se ejecute; Compass lo repara antes de invocar el CLI.

use crate::utils::{cmd, context::ProjectContext, platform};
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Entry point del CLI relativo a la raíz del proyecto.
const CLI_REL_PATH: &str = "node_modules/@tauri-apps/cli/tauri.js";

/// Entry point del CLI de Tauri dentro de `node_modules`.
///
/// Es el `bin` que declara `@tauri-apps/cli` en su `package.json`
/// (`"tauri": "./tauri.js"`). Se invoca con `node`, que es lo que hace el shim
/// `tauri.cmd` de Windows.
pub fn cli_entry(ctx: &ProjectContext) -> Option<PathBuf> {
    let entry = ctx.script_dir.join(CLI_REL_PATH);
    entry.exists().then_some(entry)
}

/// Invocación del CLI con `args`, lista para incrustar en un comando de shell.
///
/// Cae a `npx tauri` si el paquete no está, para que sea el propio npm el que
/// diga qué falta en vez de un `node: no such file` sin contexto.
///
/// El path va **relativo y sin citar** a propósito. Todos los llamadores abren la
/// ventana con el cwd en la raíz del proyecto (`current_dir` en
/// `spawn_new_window*`, `Set-Location` en las ramas PowerShell), así que
/// `node_modules/@tauri-apps/cli/tauri.js` resuelve sin problema. La alternativa
/// —ruta absoluta entrecomillada— no funciona: los comandos se lanzan como
/// `cmd /c "title X & <cmd> & pause"`, y las comillas internas desbaratan el
/// parseo de `cmd`, que no quita las suyas porque el contenido tiene caracteres
/// especiales (`&`). El resultado es que `node` recibe la ruta con un `"` pegado
/// y falla con `Cannot find module`. Con ruta relativa no hay nada que citar, y
/// por eso el separador es `/`: lo acepta `cmd` sin necesidad de escapes.
pub fn command(ctx: &ProjectContext, args: &str) -> String {
    match cli_entry(ctx) {
        Some(_) => format!("node {CLI_REL_PATH} {args}"),
        None => format!("npx tauri {args}"),
    }
}

/// ¿Está instalado el binding nativo del CLI para esta plataforma?
///
/// Comprobación de sistema de ficheros, sin lanzar un subproceso: es el caso
/// habitual tras cambiar de sistema, y merece más un mensaje explícito que un
/// *Cannot find native binding* que sale de las entrañas del CLI.
fn missing_native_binding(ctx: &ProjectContext) -> bool {
    let Some(pkg) = platform::tauri_native_pkg() else {
        return false;
    };
    let Some(dir) = pkg.strip_prefix("@tauri-apps/") else {
        return false;
    };
    !ctx.script_dir
        .join("node_modules")
        .join("@tauri-apps")
        .join(dir)
        .exists()
}

/// Garantiza que el CLI de Tauri arranca, reinstallando `node_modules` si hace
/// falta.
///
/// Hay que llamarla **antes** de construir el comando con [`command`]: si el
/// entry point no existe, `command` cae a `npx tauri` y esa caída se pierde en
/// la ventana nueva, donde no se ve el error hasta que ya no hay vuelta atrás.
pub fn ensure(ctx: &ProjectContext) -> Result<()> {
    if !ctx.has_node_modules || cli_entry(ctx).is_none() {
        println!("\n[INFO] node_modules no encontrado. Ejecutando npm install...\n");
        return cmd::run_npm_install(&ctx.script_dir).context("Falló npm install");
    }

    if missing_native_binding(ctx) {
        let pkg = platform::tauri_native_pkg().unwrap_or("el binding nativo");
        println!(
            "\n[WARN] Falta `{pkg}` en node_modules.\n\
             [INFO] Es lo que pasa cuando el proyecto se instala en un sistema y\n\
             [INFO] se usa en otro: el binding nativo es una optional dependency\n\
             [INFO] por plataforma. Reinstallando para esta máquina...\n"
        );
        cmd::run_npm_install(&ctx.script_dir).context("Falló npm install")?;
    }

    if missing_native_binding(ctx) {
        let pkg = platform::tauri_native_pkg().unwrap_or("el binding nativo");
        anyhow::bail!(
            "Sigue faltando `{pkg}` tras `npm install`.\n\
             Suele ser el bug npm/cli#4828: borra `node_modules` y `package-lock.json`\n\
             y vuelve a instalar con `npm install`."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::context::ProjectType;
    use std::path::Path;

    /// Contexto de mentira sobre un directorio temporal vacío. No hay
    /// `tempfile` en las dev-dependencies y no hace falta: cada test crea lo suyo
    /// y lo borra, y el nombre lleva el PID para que no se pisen al correr en
    /// paralelo.
    fn fake_ctx(name: &str) -> (PathBuf, ProjectContext) {
        let root = std::env::temp_dir().join(format!("cr-tauri-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ctx = ProjectContext {
            script_dir: root.clone(),
            project_type: ProjectType::Tauri,
            android_home: None,
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        };
        (root, ctx)
    }

    /// Crea el entry point del CLI, como si `@tauri-apps/cli` estuviera instalado.
    fn touch_cli_entry(root: &Path) {
        let entry = root.join(CLI_REL_PATH);
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(entry, "// stub").unwrap();
    }

    #[test]
    fn falls_back_to_npx_when_the_cli_is_not_installed() {
        let (root, ctx) = fake_ctx("no-cli");
        assert_eq!(command(&ctx, "dev"), "npx tauri dev");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invokes_node_on_the_entry_point_instead_of_the_shim() {
        let (root, ctx) = fake_ctx("with-cli");
        touch_cli_entry(&root);
        // Sin comillas: el comando acaba dentro de `cmd /c "title X & …"`, donde
        // unas comillas internas rompen el parseo (ver doc de `command`).
        assert_eq!(
            command(&ctx, "dev"),
            "node node_modules/@tauri-apps/cli/tauri.js dev"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_a_binding_installed_for_another_platform() {
        let (root, ctx) = fake_ctx("binding");
        touch_cli_entry(&root);
        // node_modules existe pero sin binding nativo: es justo lo que deja un
        // `npm install` hecho en el otro sistema.
        assert!(missing_native_binding(&ctx));

        let pkg = platform::tauri_native_pkg().expect("plataforma sin binding conocido");
        let dir = pkg.strip_prefix("@tauri-apps/").unwrap();
        std::fs::create_dir_all(root.join("node_modules/@tauri-apps").join(dir)).unwrap();
        assert!(
            !missing_native_binding(&ctx),
            "con el binding presente no debe pedir reinstallar"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}