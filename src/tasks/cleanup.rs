//! Limpieza: **una sola tarjeta** con dos niveles y un reporte de tamaños.
//!
//! Antes había dos tarjetas que se solapaban: `clean-project` era en la práctica
//! la variante "agresiva" de `clean-cache`, y en Flutter/KMP borraban casi los
//! mismos directorios. Revisando de cerca, además, tenían fallos reales:
//!
//! - La "agresiva" de **Kotlin** borraba **menos** que la rápida: se dejaba
//!   `build/`, `<módulo>/build/` y `.kotlin/`.
//! - La "agresiva" de **Rust** no hacía nada extra: borraba `target/` y después
//!   lanzaba `cargo clean`, ya sobre un directorio vacío.
//! - `clean_project` tenía pasos **muertos por orden**: borraba `target/` y luego
//!   `target/.cargo-lock`; `node_modules/` y luego `node_modules/.vite`; y
//!   `gen/android/app/build` en el paso 2 para volver a borrarlo en el paso 8.
//!   Su `ProgressBar` contaba 8 pasos cuando sólo 5 hacían algo, y el tamaño de
//!   `gen/` no se sumaba al total liberado.
//! - Los `remove_dir(...).ok()` se tragaban los errores: un archivo bloqueado
//!   (común en Windows con un `target/` en uso) se reportaba como `[OK]` sin
//!   haber borrado nada.
//!
//! Ahora hay dos niveles reales:
//!
//! - **Rápida** (`quick_clean`): borra builds y caches baratos de regenerar y
//!   **conserva las dependencias** (`node_modules/`, `target/` completo).
//! - **Completa** (`full_clean`): delega en el clean por tipo
//!   (`clean_project`/`flutter::clean`/`kotlin::clean`/`rust::clean`), que
//!   además borra dependencias y deja el proyecto listo para portar.
//! - **Ver tamaños** (`report_sizes`): mide sin borrar.

use std::path::{Path, PathBuf};

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{MultiSelect, Select};

use crate::utils::{
    cmd,
    context::{kotlin_android_dir, ProjectContext, ProjectType},
    ui,
};

// ───────────────────────────────────────────────────────────────────────────
// Utilidades
// ───────────────────────────────────────────────────────────────────────────

fn banner(title: &str) {
    // Igual que `ui::banner`: se cuenta por caracteres (no bytes) para que el
    // centrado no se desajuste. Títulos en ASCII ⇒ ancho de columna = nº chars.
    let width = title.chars().count().max(48);
    let pad = width - title.chars().count();
    let left = pad / 2;
    let right = pad - left;
    println!("╔{}╗", "═".repeat(width));
    println!("║{}{}{}║", " ".repeat(left), title, " ".repeat(right));
    println!("╚{}╝", "═".repeat(width));
}

fn format_size(bytes: u64) -> String {
    if bytes > 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes > 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    }
}

/// Tamaño en disco de un archivo o directorio (recursivo). 0 si no existe.
fn dir_size(path: &Path) -> u64 {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.is_file() {
            return meta.len();
        }
    }
    fn walk(dir: &Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    total += walk(&path);
                } else if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
        total
    }
    walk(path)
}

/// Borra `path` (directorio o archivo) y lo reporta. Devuelve los bytes que
/// ocupaba.
///
/// A diferencia de los `remove_dir(...).ok()` de antes, un fallo se muestra:
/// borrar un `target/` en uso en Windows fallaba en silencio y se reportaba
/// como `[OK]`.
fn purge(label: &str, path: &Path) -> u64 {
    if !path.exists() {
        println!("  [--] No hay {}", label);
        return 0;
    }
    let size = dir_size(path);
    let result: Result<()> = if path.is_dir() {
        cmd::remove_dir(path)
    } else {
        std::fs::remove_file(path).map_err(anyhow::Error::from)
    };
    match result {
        Ok(()) => println!("  [OK] {} ({})", label, format_size(size)),
        Err(e) => {
            println!("  [WARN] No se pudo borrar {} ({})", label, e);
            return 0;
        }
    }
    size
}

/// Todos los directorios llamados `name` bajo `root`, a cualquier profundidad.
///
/// La profundidad está acotada para no recorrer un árbol enorme por error.
fn find_dirs_named(root: &Path, name: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, name: &str, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            if entry.file_name() == std::ffi::OsStr::new(name) {
                out.push(p.clone());
            } else {
                walk(&p, name, depth + 1, out);
            }
        }
    }

    let mut out = Vec::new();
    if root.is_dir() {
        walk(root, name, 0, &mut out);
    }
    out
}

fn module_name(module_dir: &Path) -> String {
    module_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "composeApp".to_string())
}

// ───────────────────────────────────────────────────────────────────────────
// Planes de limpieza (una sola fuente de verdad de "qué se borra")
// ───────────────────────────────────────────────────────────────────────────

/// Limpieza **rápida**: builds y caches baratos de regenerar. Nunca incluye
/// `node_modules/` ni el `target/` completo.
fn quick_targets(ctx: &ProjectContext) -> Vec<(String, PathBuf)> {
    let root = &ctx.script_dir;
    match ctx.project_type {
        ProjectType::Tauri => vec![
            ("Vite cache".into(), root.join("node_modules/.vite")),
            ("dist/".into(), root.join("dist")),
            ("target/debug".into(), root.join("src-tauri/target/debug")),
            ("target/release".into(), root.join("src-tauri/target/release")),
            (
                "gen Android build".into(),
                root.join("src-tauri/gen/android/app/build"),
            ),
        ],
        ProjectType::Flutter => vec![
            ("build/".into(), root.join("build")),
            (".dart_tool/".into(), root.join(".dart_tool")),
            ("android/app/build".into(), root.join("android/app/build")),
        ],
        ProjectType::Kotlin => {
            let module = kotlin_android_dir(ctx);
            let name = module_name(&module);
            vec![
                ("build/ raíz".into(), root.join("build")),
                (format!("{}/build", name), module.join("build")),
                (".gradle/".into(), root.join(".gradle")),
                (".kotlin/".into(), root.join(".kotlin")),
            ]
        }
        ProjectType::Rust => vec![
            ("target/debug".into(), root.join("target/debug")),
            ("target/release".into(), root.join("target/release")),
        ],
    }
}

/// Limpieza **completa**: incluye dependencias y deja el proyecto portátil.
/// Sólo se usa para el reporte de tamaños; el borrado real lo hacen los `clean()`
/// por tipo, reutilizando la lógica que ya existía.
fn full_targets(ctx: &ProjectContext) -> Vec<(String, PathBuf)> {
    let root = &ctx.script_dir;
    match ctx.project_type {
        ProjectType::Tauri => {
            let gen = root.join("src-tauri/gen");
            vec![
                ("src-tauri/target/".into(), root.join("src-tauri/target")),
                ("node_modules/".into(), root.join("node_modules")),
                ("dist/".into(), root.join("dist")),
                (
                    "gen Android build".into(),
                    gen.join("android/app/build"),
                ),
                ("gen Android .gradle".into(), gen.join("android/.gradle")),
                ("gen Android build dir".into(), gen.join("android/build")),
                ("gen Android gradle".into(), gen.join("android/gradle")),
                ("gen Android buildSrc".into(), gen.join("android/buildSrc")),
                ("gen schemas".into(), gen.join("schemas")),
            ]
        }
        ProjectType::Flutter => vec![
            ("build/".into(), root.join("build")),
            (".dart_tool/".into(), root.join(".dart_tool")),
            ("android/.gradle".into(), root.join("android/.gradle")),
            ("android/build".into(), root.join("android/build")),
            ("android/app/build".into(), root.join("android/app/build")),
            ("node_modules/".into(), root.join("node_modules")),
        ],
        ProjectType::Kotlin => {
            let module = kotlin_android_dir(ctx);
            let name = module_name(&module);
            vec![
                ("build/ raíz".into(), root.join("build")),
                (format!("{}/build", name), module.join("build")),
                (".gradle/".into(), root.join(".gradle")),
                (".kotlin/".into(), root.join(".kotlin")),
                ("node_modules/".into(), root.join("node_modules")),
            ]
        }
        ProjectType::Rust => vec![("target/".into(), root.join("target"))],
    }
}

/// Un candidato del modo interactivo: qué se puede borrar y si es dependencia.
struct Candidate {
    label: String,
    path: PathBuf,
    /// `true` = dependencias instaladas; restaurarlas tarda (npm/cargo/pub).
    deps: bool,
}

/// Candidatos del modo interactivo: **sin solapamientos** (ningún candidato
/// está dentro de otro, para que los tamaños no se cuenten dos veces).
///
/// A diferencia de `quick_targets`, aquí se ofrece el `target/` entero en vez de
/// `target/debug`+`target/release`, y no se listan subcarpetas de `node_modules/`.
fn selectable_targets(ctx: &ProjectContext) -> Vec<Candidate> {
    let root = &ctx.script_dir;
    let c = |label: &str, path: PathBuf, deps: bool| Candidate {
        label: label.to_string(),
        path,
        deps,
    };

    match ctx.project_type {
        ProjectType::Tauri => {
            let gen = root.join("src-tauri/gen");
            vec![
                c("src-tauri/target/", root.join("src-tauri/target"), true),
                c("node_modules/", root.join("node_modules"), true),
                c("dist/", root.join("dist"), false),
                c("gen Android build", gen.join("android/app/build"), false),
                c("gen Android .gradle", gen.join("android/.gradle"), false),
                c("gen Android build dir", gen.join("android/build"), false),
                c("gen Android gradle", gen.join("android/gradle"), false),
                c("gen Android buildSrc", gen.join("android/buildSrc"), false),
                c("gen schemas", gen.join("schemas"), false),
            ]
        }
        ProjectType::Flutter => vec![
            c("build/", root.join("build"), false),
            c(".dart_tool/", root.join(".dart_tool"), false),
            c("android/.gradle", root.join("android/.gradle"), false),
            c("android/build", root.join("android/build"), false),
            c("android/app/build", root.join("android/app/build"), false),
            c("node_modules/", root.join("node_modules"), true),
        ],
        ProjectType::Kotlin => {
            let module = kotlin_android_dir(ctx);
            let name = module_name(&module);
            vec![
                c("build/ raíz", root.join("build"), false),
                c(&format!("{}/build", name), module.join("build"), false),
                c(".gradle/", root.join(".gradle"), false),
                c(".kotlin/", root.join(".kotlin"), false),
                c("node_modules/", root.join("node_modules"), true),
            ]
        }
        ProjectType::Rust => vec![c("target/", root.join("target"), true)],
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Acciones
// ───────────────────────────────────────────────────────────────────────────

fn quick_clean(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    banner("LIMPIEZA RÁPIDA");
    println!();
    println!("Borra builds y caches regenerables; conserva node_modules/ y");
    println!("las dependencias. Nada que tardes minutos en reinstalar.");
    println!();

    let mut freed = 0u64;
    for (label, path) in quick_targets(ctx) {
        freed += purge(&label, &path);
    }

    println!();
    println!("[OK] Limpieza rápida completada. Liberado: {}", format_size(freed));
    ui::pause();
    Ok(())
}

fn full_clean(ctx: &ProjectContext) -> Result<()> {
    // Se reutilizan los `clean()` por tipo que ya existían: en Tauri el de
    // portabilidad y en Flutter/KMP el suyo. No se duplica la lógica.
    match ctx.project_type {
        ProjectType::Tauri => clean_project(ctx),
        ProjectType::Flutter => crate::tasks::flutter::clean(ctx),
        ProjectType::Kotlin => crate::tasks::kotlin::clean(ctx),
        ProjectType::Rust => crate::tasks::rust::clean(ctx),
    }
}

fn report_sizes(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    banner("ESPACIO QUE SE PUEDE LIBERAR");
    println!();
    println!("Nada se borra aquí: sólo se mide.\n");

    println!("── Rápida (opción 1) ──────────────────────────────");
    let mut quick_total = 0u64;
    for (label, path) in quick_targets(ctx) {
        let size = dir_size(&path);
        if path.exists() {
            println!("  {:<26} {:>10}", label, format_size(size));
            quick_total += size;
        } else {
            println!("  {:<26} {:>10}", label, "—");
        }
    }
    println!("  {:<26} {:>10}", "SUBTOTAL", format_size(quick_total));

    println!();
    println!("── Completa (opción 2) ────────────────────────────");
    let mut full_total = 0u64;
    for (label, path) in full_targets(ctx) {
        let size = dir_size(&path);
        if path.exists() {
            println!("  {:<26} {:>10}", label, format_size(size));
            full_total += size;
        } else {
            println!("  {:<26} {:>10}", label, "—");
        }
    }
    println!("  {:<26} {:>10}", "TOTAL", format_size(full_total));

    println!();
    println!("[TIP] La rápida conserva dependencias; la completa las borra, así que");
    println!("      restaurarlas después (npm install / cargo build / pub get) tarda más.");
    ui::pause();
    Ok(())
}

/// Dry-run interactivo: lista lo que existe y deja **elegir** qué borrar.
///
/// Sólo muestra candidatos que existen (no tiene sentido marcar lo que no está)
/// y avisa aparte de las dependencias, porque restaurarlas tarda.
fn choose_and_purge(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    banner("ELEGIR QUÉ BORRAR");
    println!();
    println!("Marca con Espacio lo que quieras eliminar y confirma con Enter.");
    println!();

    let candidates: Vec<Candidate> = selectable_targets(ctx)
        .into_iter()
        .filter(|c| c.path.exists())
        .collect();

    if candidates.is_empty() {
        println!("[INFO] No hay nada que borrar: no existe ningún directorio de build.");
        ui::pause();
        return Ok(());
    }

    // El texto mostrado es la clave de vuelta al candidato; las etiquetas son
    // únicas, así que la búsqueda por texto es inequívoca.
    let rendered: Vec<String> = candidates
        .iter()
        .map(|c| {
            let tag = if c.deps { "deps " } else { "build" };
            format!(
                "{:<24} {:>10}  [{}]",
                c.label,
                format_size(dir_size(&c.path)),
                tag
            )
        })
        .collect();

    let selected = MultiSelect::new("¿Qué borro?", rendered.clone())
        .with_vim_mode(true)
        .with_help_message("Espacio marca/desmarca · Enter borra lo marcado · Esc cancela")
        .prompt()
        .unwrap_or_default();

    if selected.is_empty() {
        println!("\n[INFO] No seleccionaste nada.");
        ui::pause();
        return Ok(());
    }

    // Confirmación extra sólo si hay dependencias: son las caras de restaurar.
    let deps: Vec<&Candidate> = selected
        .iter()
        .filter_map(|s| rendered.iter().position(|r| r == s))
        .map(|i| &candidates[i])
        .filter(|c| c.deps)
        .collect();
    if !deps.is_empty() {
        println!("\n[AVISO] Vas a borrar dependencias; reinstalarlas tardará:");
        for c in &deps {
            println!("  - {}", c.label);
        }
        let ok = inquire::Confirm::new("¿Continuar?")
            .with_default(false)
            .prompt()?;
        if !ok {
            println!("[INFO] Operación cancelada.");
            ui::pause();
            return Ok(());
        }
    }

    // Los candidatos no se solapan, así que se puede borrar en cualquier orden.
    println!();
    let mut freed = 0u64;
    for opt in &selected {
        if let Some(i) = rendered.iter().position(|r| r == opt) {
            freed += purge(&candidates[i].label, &candidates[i].path);
        }
    }

    println!();
    println!("[OK] Limpieza selectiva completada. Liberado: {}", format_size(freed));
    ui::pause();
    Ok(())
}

/// Menú de limpieza unificado (absorbe las antiguas tarjetas Clean Cache y
/// Clean Project).
pub fn manage(ctx: &ProjectContext) -> Result<()> {
    loop {
        cmd::clear_screen();
        banner("LIMPIEZA");
        println!();
        let options = vec![
            "1. Rápida — borra builds y caches (conserva dependencias)",
            "2. Completa — borra además dependencias (node_modules, target)",
            "3. Elegir qué borrar (lista y marca)",
            "4. Ver tamaños (sin borrar nada)",
            "5. Volver al menú principal",
        ];
        let choice = Select::new("Selecciona tipo de limpieza:", options)
            .with_vim_mode(true)
            .prompt()?;

        match choice {
            s if s.starts_with("1.") => quick_clean(ctx)?,
            s if s.starts_with("2.") => full_clean(ctx)?,
            s if s.starts_with("3.") => choose_and_purge(ctx)?,
            s if s.starts_with("4.") => report_sizes(ctx)?,
            _ => break,
        }
    }
    Ok(())
}

/// Limpieza completa de un proyecto Tauri, pensada para **portabilidad**: deja
/// fuentes y configuración, borra todo lo generado.
///
/// Orden corregido: se borra `target/` una vez (no se re-visita `.cargo-lock`
/// dentro de él), `node_modules/` una vez (no se re-visita `.vite` dentro) y
/// `gen/android/app/build` una vez.
fn clean_project(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    banner("LIMPIAR PROYECTO PARA PORTABILIDAD");
    println!();
    println!("Se eliminarán TODOS los archivos de compilación y dependencias,");
    println!("dejando sólo el código fuente y la configuración.");
    println!();
    println!("Se eliminará:");
    println!("  - src-tauri/target/");
    println!("  - src-tauri/gen/android/ (builds, gradle, auto-generados)");
    println!("  - src-tauri/gen/schemas/");
    println!("  - node_modules/");
    println!("  - dist/");
    println!("  - src-tauri/build_log.txt");
    println!();
    println!("Se conservará:");
    println!("  - src/ (frontend)");
    println!("  - src-tauri/src/ (Rust)");
    println!("  - src-tauri/Cargo.toml + Cargo.lock");
    println!("  - src-tauri/tauri.conf.json, build.rs, icons/, capabilities/");
    println!("  - package.json + package-lock.json");
    println!("  - vite.config.ts, tsconfig*.json, index.html");
    println!("  - public/");
    println!("  - AndroidManifest.xml, *.kt, res/");
    println!("  - Firmas: keystore (*.keystore, keystore.properties)");
    println!("  - Android personalizado: assets/, jniLibs/");
    println!("  - WhatsApp: src-tauri/whatsapp-engine/ + resources/whatsapp-engine-bundle/");
    println!();

    let confirm = inquire::Confirm::new("¿Estás seguro?")
        .with_default(false)
        .prompt()?;

    if !confirm {
        println!("[INFO] Operación cancelada.");
        ui::pause();
        return Ok(());
    }

    let pb = ProgressBar::new(5);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40}] {pos}/{len} {msg}")?
            .progress_chars("=> "),
    );

    let mut total_freed = 0u64;

    // 1. Rust build artifacts (arrastra también target/.cargo-lock).
    pb.set_message("Limpiando target/");
    total_freed += purge("target/", &ctx.script_dir.join("src-tauri/target"));
    pb.inc(1);

    // 2. Android auto-generado (incluye app/build).
    pb.set_message("Limpiando gen/ (preservando config manual)");
    let gen_dir = ctx.script_dir.join("src-tauri/gen");
    if gen_dir.exists() {
        let size = dir_size(&gen_dir);
        let artifacts = [
            "android/app/build",
            "android/.gradle",
            "android/build",
            "android/gradle",
            "android/buildSrc",
            "android/app/proguard-rules.pro",
            "android/app/proguard-tauri.pro",
            "android/app/tauri.build.gradle.kts",
            "android/app/tauri.properties",
            "schemas",
        ];
        for rel in &artifacts {
            let full = gen_dir.join(rel);
            if full.exists() {
                let _ = if full.is_dir() {
                    cmd::remove_dir(&full)
                } else {
                    std::fs::remove_file(&full).map_err(anyhow::Error::from)
                };
            }
        }
        // Los `generated` del código Kotlin/Java (protobuf, data binding) viven
        // bajo el paquete de la app, cuyo nombre depende del `identifier` del
        // proyecto. Se barren por nombre de carpeta en vez de hardcodear una ruta
        // que sólo tenía sentido para un proyecto concreto.
        for generated in find_dirs_named(&gen_dir.join("android/app/src/main/java"), "generated") {
            cmd::remove_dir(&generated).ok();
        }
        total_freed += size;
        println!("  [OK] gen/ ({})", format_size(size));
    } else {
        println!("  [--] No hay gen/");
    }
    pb.inc(1);

    // 3. node_modules (arrastra node_modules/.vite).
    pb.set_message("Eliminando node_modules/");
    total_freed += purge("node_modules/", &ctx.script_dir.join("node_modules"));
    pb.inc(1);

    // 4. dist
    pb.set_message("Eliminando dist/");
    total_freed += purge("dist/", &ctx.script_dir.join("dist"));
    pb.inc(1);

    // 5. Logs de build
    pb.set_message("Eliminando logs");
    total_freed += purge("build_log.txt", &ctx.script_dir.join("src-tauri/build_log.txt"));
    pb.inc(1);

    pb.finish_with_message("Limpieza completada");

    println!();
    println!("[OK] Proyecto limpiado para portabilidad!");
    println!("     Espacio liberado: {}", format_size(total_freed));
    println!();
    println!("Para compilar en una máquina nueva:");
    println!("  1. npm install");
    println!("  2. npx tauri android init  (solo para Android)");
    println!("  3. npm run tauri dev          (Desktop)");
    println!("  4. npm run tauri android dev  (Android)");

    ui::pause();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_ctx(project_type: ProjectType) -> ProjectContext {
        ProjectContext {
            script_dir: PathBuf::from("/tmp/compass-clean-test"),
            project_type,
            android_home: None,
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        }
    }

    /// La rápida nunca debe borrar dependencias instaladas: si incluyera
    /// `node_modules/` o todo `target/`, dejaría de ser "rápida".
    #[test]
    fn quick_clean_never_removes_installed_dependencies() {
        for pt in [
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::Kotlin,
            ProjectType::Rust,
        ] {
            for (label, path) in quick_targets(&fake_ctx(pt)) {
                let p = path.to_string_lossy().replace('\\', "/");
                assert!(
                    !p.ends_with("/node_modules") && p != "node_modules",
                    "{pt:?}: la rápida no debe borrar node_modules/ (entrada '{label}')"
                );
                assert!(
                    !p.ends_with("/target") && p != "target" && !p.ends_with("src-tauri/target"),
                    "{pt:?}: la rápida no debe borrar todo target/ (entrada '{label}')"
                );
            }
        }
    }

    /// La completa sí cubre el `target/` entero, sea cual sea el tipo.
    #[test]
    fn full_clean_is_a_superset_for_rust() {
        let ctx = fake_ctx(ProjectType::Rust);
        assert!(full_targets(&ctx)
            .iter()
            .any(|(_, p)| p.to_string_lossy().ends_with("target")));
        assert_eq!(quick_targets(&ctx).len(), 2, "rápida = target/debug + target/release");
    }

    /// Kotlin: el nombre del módulo sale de `kotlin_android_dir`, no hardcodeado.
    #[test]
    fn kotlin_plan_uses_the_detected_module_name() {
        let ctx = fake_ctx(ProjectType::Kotlin);
        // Sin `app/` ni `composeApp/` en disco, `kotlin_android_dir` cae a
        // `composeApp`; lo que importa es que el plan use esa misma función.
        let expected = module_name(&kotlin_android_dir(&ctx));
        let labels: Vec<String> = quick_targets(&ctx).into_iter().map(|(l, _)| l).collect();
        assert!(labels.iter().any(|l| l == &format!("{expected}/build")));
    }

    /// Los candidatos del modo interactivo no deben solaparse: si uno estuviera
    /// dentro de otro, su tamaño se contaría dos veces en el reporte.
    #[test]
    fn interactive_candidates_do_not_overlap() {
        for pt in [
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::Kotlin,
            ProjectType::Rust,
        ] {
            let cands = selectable_targets(&fake_ctx(pt));
            for a in &cands {
                for b in &cands {
                    if a.label == b.label {
                        continue;
                    }
                    assert!(
                        !b.path.starts_with(&a.path),
                        "{pt:?}: '{}' está dentro de '{}'",
                        b.label,
                        a.label
                    );
                }
            }
        }
    }

    /// Las etiquetas del modo interactivo han de ser únicas: `choose_and_purge`
    /// mapea la opción marcada de vuelta al candidato buscando por texto.
    #[test]
    fn interactive_candidate_labels_are_unique() {
        for pt in [
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::Kotlin,
            ProjectType::Rust,
        ] {
            let mut labels: Vec<String> = selectable_targets(&fake_ctx(pt))
                .into_iter()
                .map(|c| c.label)
                .collect();
            let total = labels.len();
            labels.sort();
            labels.dedup();
            assert_eq!(labels.len(), total, "{pt:?}: etiquetas repetidas");
        }
    }

    /// `node_modules/` y `target/` deben marcarse como dependencias (caras de
    /// restaurar); `dist/` o `build/` no.
    #[test]
    fn dependency_candidates_are_flagged() {
        let tauri = selectable_targets(&fake_ctx(ProjectType::Tauri));
        assert!(tauri.iter().any(|c| c.label == "node_modules/" && c.deps));
        assert!(tauri.iter().any(|c| c.label == "dist/" && !c.deps));

        let rust = selectable_targets(&fake_ctx(ProjectType::Rust));
        assert!(rust.iter().all(|c| c.deps), "en Rust target/ es la dependencia");
    }
}
