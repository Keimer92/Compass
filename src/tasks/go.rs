use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::utils::{cmd, context::ProjectContext, ui};

const WINDOWS_ICON_RESOURCE: &[u8] = include_bytes!("../../assets/icon/golang_windows_amd64.syso");
const WINDOWS_ICON_FILENAME: &str = "compass_go_icon_windows_amd64.syso";

pub fn run(ctx: &ProjectContext) -> Result<()> {
    with_windows_icon(ctx, || {
        cmd::run_in_current_window("go run .", &ctx.script_dir)
    })
}

pub fn reload(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Iniciando Air para reconstruir y reiniciar al cambiar archivos Go...");
    with_windows_icon(ctx, || {
        cmd::run_in_dir_status("air", &[], &ctx.script_dir).with_context(|| {
            "No se pudo ejecutar Air. Instálalo con `go install github.com/air-verse/air@latest` y asegúrate de que GOPATH\\bin esté en PATH."
        })
    })
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    let output_dir = ctx.script_dir.join("bin");
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("No se pudo crear {}", output_dir.display()))?;
    let output_name = executable_name(&ctx.script_dir);
    let command = format!("go build -o bin/{output_name} .");
    with_windows_icon(ctx, || {
        cmd::run_build_and_reveal(&command, &ctx.script_dir, &output_dir, &["exe"], None)
    })
}

pub fn test(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Ejecutando pruebas Go: go test ./...\n");
    cmd::run_in_current_window("go test ./...", &ctx.script_dir)
}

pub fn clean(ctx: &ProjectContext) -> Result<()> {
    println!("[INFO] Ejecutando go clean...");
    cmd::run_in_dir_status("go", &["clean"], &ctx.script_dir)?;
    let bin = ctx.script_dir.join("bin");
    if bin.is_dir() {
        cmd::remove_dir(&bin)?;
    }
    println!("[OK] Limpieza Go completada.");
    ui::pause();
    Ok(())
}

fn executable_name(root: &Path) -> String {
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let safe: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("{}.exe", if safe.is_empty() { "app" } else { &safe })
}

struct GoIconResource {
    path: PathBuf,
    installed: bool,
}

impl GoIconResource {
    fn install(root: &Path) -> Result<Self> {
        let path = root.join(WINDOWS_ICON_FILENAME);
        remove_stale_icon_resource(&path)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "No se pudo crear el recurso temporal del icono Go en {}",
                    path.display()
                )
            })?;
        let resource = Self {
            path,
            installed: true,
        };
        file.write_all(WINDOWS_ICON_RESOURCE)
            .context("No se pudo escribir el recurso temporal del icono Go")?;
        Ok(resource)
    }

    fn remove(mut self) -> Result<()> {
        fs::remove_file(&self.path)
            .with_context(|| format!("No se pudo quitar {}", self.path.display()))?;
        self.installed = false;
        Ok(())
    }
}

fn remove_stale_icon_resource(path: &Path) -> Result<()> {
    match fs::read(path) {
        Ok(contents) if contents == WINDOWS_ICON_RESOURCE => {
            fs::remove_file(path).with_context(|| {
                format!(
                    "No se pudo quitar el recurso Go anterior: {}",
                    path.display()
                )
            })
        }
        Ok(_) => anyhow::bail!(
            "Ya existe un archivo en la ruta reservada para el icono temporal Go: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "No se pudo comprobar el recurso Go anterior: {}",
                path.display()
            )
        }),
    }
}

impl Drop for GoIconResource {
    fn drop(&mut self) {
        if self.installed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn with_windows_icon<T>(ctx: &ProjectContext, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let resource = GoIconResource::install(&ctx.script_dir)?;
    let result = action();
    let cleanup = resource.remove();
    match (result, cleanup) {
        (Err(error), Err(cleanup_error)) => Err(error.context(format!(
            "Además, no se pudo limpiar el recurso temporal: {cleanup_error:#}"
        ))),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("compass-go-icon-{unique}"));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn executable_name_is_windows_safe_and_not_empty() {
        assert_eq!(
            executable_name(Path::new(r"D:\Projects\Go + Wails")),
            "Go___Wails.exe"
        );
        assert_eq!(executable_name(Path::new("")), "app.exe");
    }

    #[test]
    fn icon_resource_is_installed_and_removed() {
        let root = temp_dir();
        let path = root.join(WINDOWS_ICON_FILENAME);
        let resource = GoIconResource::install(&root).unwrap();
        assert_eq!(fs::read(&path).unwrap(), WINDOWS_ICON_RESOURCE);

        resource.remove().unwrap();
        assert!(!path.exists());
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn stale_icon_resource_is_replaced() {
        let root = temp_dir();
        let path = root.join(WINDOWS_ICON_FILENAME);
        fs::write(&path, WINDOWS_ICON_RESOURCE).unwrap();

        let resource = GoIconResource::install(&root).unwrap();
        assert_eq!(fs::read(&path).unwrap(), WINDOWS_ICON_RESOURCE);

        resource.remove().unwrap();
        assert!(!path.exists());
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn icon_resource_does_not_overwrite_an_unrelated_file() {
        let root = temp_dir();
        let path = root.join(WINDOWS_ICON_FILENAME);
        fs::write(&path, b"existing resource").unwrap();

        assert!(GoIconResource::install(&root).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"existing resource");

        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
