use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::utils::{context::ProjectContext, ui};

fn interpreter(ctx: &ProjectContext) -> PathBuf {
    let venv_python = ctx
        .script_dir
        .join(".venv")
        .join("Scripts")
        .join("python.exe");
    if venv_python.is_file() {
        venv_python
    } else {
        PathBuf::from("python.exe")
    }
}

fn inferred_script(root: &Path) -> Option<&'static str> {
    let main = root.join("main.py").is_file();
    let app = root.join("app.py").is_file();
    match (main, app) {
        (true, false) => Some("main.py"),
        (false, true) => Some("app.py"),
        _ => None,
    }
}

fn run_process(ctx: &ProjectContext, args: &[String], purpose: &str) -> Result<()> {
    let python = interpreter(ctx);
    println!("[INFO] {purpose}: {} {}", python.display(), args.join(" "));
    let status = Command::new(&python)
        .args(args)
        .current_dir(&ctx.script_dir)
        .status()
        .with_context(|| {
            format!(
                "No se pudo ejecutar Python desde '{}'. Instala Python o crea el entorno .venv del proyecto.",
                python.display()
            )
        })?;
    println!();
    ui::pause();
    if !status.success() {
        anyhow::bail!("El proceso Python terminó con código: {status}");
    }
    Ok(())
}

fn selected_entry(ctx: &ProjectContext) -> Result<String> {
    if let Some(script) = inferred_script(&ctx.script_dir) {
        return Ok(script.to_string());
    }
    let target = ui::ask_text(
        "Script .py o módulo Python (por ejemplo src/main.py o paquete.cli)",
        None,
    )?;
    let target = target.trim().to_string();
    if target.is_empty() {
        anyhow::bail!("No se indicó un script o módulo Python.");
    }
    Ok(target)
}

pub fn run(ctx: &ProjectContext) -> Result<()> {
    let entry = selected_entry(ctx)?;
    let args = if entry.ends_with(".py") {
        let path = ctx.script_dir.join(&entry);
        if !path.is_file() {
            anyhow::bail!("No existe el script Python: {}", path.display());
        }
        vec![entry]
    } else {
        vec!["-m".to_string(), entry]
    };
    run_process(ctx, &args, "Ejecutando Python")
}

/// Recarga de servidor ASGI mediante el watcher integrado de Uvicorn.
/// Reinicia el proceso de aplicación; no conserva el estado en memoria.
pub fn reload(ctx: &ProjectContext) -> Result<()> {
    let target = ui::ask_text(
        "Aplicación ASGI para Uvicorn (formato módulo:variable)",
        Some("main:app"),
    )?;
    let target = target.trim();
    if target.is_empty() || !target.contains(':') {
        anyhow::bail!("El destino ASGI debe tener el formato módulo:variable.");
    }
    run_process(
        ctx,
        &[
            "-m".to_string(),
            "uvicorn".to_string(),
            target.to_string(),
            "--reload".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
        ],
        "Iniciando Uvicorn con autorecarga",
    )
}

pub fn test(ctx: &ProjectContext) -> Result<()> {
    run_process(
        ctx,
        &["-m".into(), "pytest".into()],
        "Ejecutando pruebas con pytest",
    )
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    run_process(
        ctx,
        &["-m".into(), "build".into()],
        "Construyendo wheel y sdist",
    )
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    let root = &ctx.script_dir;
    for (label, path) in [
        ("build/", root.join("build")),
        ("dist/", root.join("dist")),
        ("__pycache__/", root.join("__pycache__")),
        (".pytest_cache/", root.join(".pytest_cache")),
        (".mypy_cache/", root.join(".mypy_cache")),
        (".ruff_cache/", root.join(".ruff_cache")),
        (".venv/", root.join(".venv")),
        ("venv/", root.join("venv")),
    ] {
        if path.exists() {
            if path.is_dir() {
                crate::utils::cmd::remove_dir(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
            println!("[OK] Eliminado {label}");
        }
    }
    for name in ["__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"] {
        for path in find_dirs_named(root, name) {
            crate::utils::cmd::remove_dir(&path)?;
            println!("[OK] Eliminado {}", path.display());
        }
    }
    println!("[OK] Limpieza Python completada.");
    Ok(())
}

pub(crate) fn find_dirs_named(root: &Path, name: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, name: &str, depth: usize, found: &mut Vec<PathBuf>) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let entry_name = entry.file_name();
            if entry_name == ".venv" || entry_name == "venv" || entry_name == ".git" {
                continue;
            }
            if entry_name == name {
                found.push(path);
            } else {
                walk(&path, name, depth + 1, found);
            }
        }
    }

    let mut found = Vec::new();
    walk(root, name, 0, &mut found);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_is_inferred_only_when_unambiguous() {
        let dir = std::env::temp_dir().join(format!("compass-python-entry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.py"), "").unwrap();
        assert_eq!(inferred_script(&dir), Some("main.py"));
        std::fs::write(dir.join("app.py"), "").unwrap();
        assert_eq!(inferred_script(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_search_does_not_enter_virtual_environments() {
        let dir = std::env::temp_dir().join(format!("compass-python-cache-{}", std::process::id()));
        let cache = dir.join(".venv").join("Lib").join("__pycache__");
        std::fs::create_dir_all(&cache).unwrap();
        assert!(find_dirs_named(&dir, "__pycache__").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
