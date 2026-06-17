use std::io::{self, Write};
use anyhow::Result;
use crate::utils::{cmd, context::ProjectContext};

pub fn init(ctx: &ProjectContext) -> Result<()> {
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      INICIALIZAR REPOSITORIO GIT (main)          ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let git_dir = ctx.script_dir.join(".git");

    if git_dir.exists() {
        // Check if there are already commits
        let has_commits = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&ctx.script_dir)
            .output()
            .ok()
            .map_or(false, |o| o.status.success());

        if has_commits {
            println!("[INFO] El repositorio ya existe y tiene commits.");
            println!("  No se realizó ningún cambio.");
            pause();
            return Ok(());
        }
    }

    println!("[INFO] Inicializando repositorio git...");
    let output = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&ctx.script_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[ERROR] git init falló:\n{}", stderr);
        pause();
        return Ok(());
    }

    println!("[OK] Repositorio inicializado.");

    // Check if default branch is already 'main'
    let default_branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&ctx.script_dir)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string());

    if default_branch.as_deref() != Some("main") {
        println!("[INFO] Renombrando rama a 'main'...");
        std::process::Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(&ctx.script_dir)
            .output()?;
        println!("[OK] Rama renombrada a 'main'.");
    } else {
        println!("[OK] La rama ya se llama 'main'.");
    }

    println!("[INFO] Agregando archivos al staging...");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&ctx.script_dir)
        .output()?;
    println!("[OK] Archivos agregados.");

    println!("[INFO] Creando primer commit...");
    let output = std::process::Command::new("git")
        .args(["commit", "-m", "First Commit"])
        .current_dir(&ctx.script_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("nothing to commit") || stderr.contains("no changes") {
            println!("[INFO] No hay archivos para commitear (posiblemente ya commiteados).");
        } else {
            eprintln!("[WARN] git commit emitió:\n{}", stderr);
        }
    } else {
        println!("[OK] Primer commit creado: 'First Commit'.");
    }

    println!();
    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
