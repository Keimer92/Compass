use std::path::Path;

use anyhow::{Context, Result};

use crate::utils::{cmd, context::ProjectContext, ui};

pub fn run(ctx: &ProjectContext) -> Result<()> {
    cmd::run_in_current_window("go run .", &ctx.script_dir)
}

pub fn build(ctx: &ProjectContext) -> Result<()> {
    let output_dir = ctx.script_dir.join("bin");
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("No se pudo crear {}", output_dir.display()))?;
    let output_name = executable_name(&ctx.script_dir);
    let command = format!("go build -o bin/{output_name} .");
    cmd::run_build_and_reveal(&command, &ctx.script_dir, &output_dir, &["exe"], None)?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_name_is_windows_safe_and_not_empty() {
        assert_eq!(
            executable_name(Path::new(r"D:\Projects\Go + Wails")),
            "Go___Wails.exe"
        );
        assert_eq!(executable_name(Path::new("")), "app.exe");
    }
}
