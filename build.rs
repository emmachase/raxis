//! Build script for raxis - compiles HLSL shaders for custom effects.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Only compile shaders on Windows
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let shader_dir = Path::new("src/gfx/effects/shaders");

    // List of shaders to compile
    let shaders = ["box_blur", "liquid_glass"];

    // Find fxc.exe
    let fxc_path = find_fxc().expect(
        "Could not find fxc.exe. Please install the Windows SDK or set FXC_PATH environment variable.",
    );

    println!("cargo:rerun-if-env-changed=FXC_PATH");

    for shader_name in &shaders {
        let hlsl_path = shader_dir.join(format!("{}.hlsl", shader_name));
        let cso_path = shader_dir.join(format!("{}.cso", shader_name));

        // Tell cargo to rerun if the shader source changes
        println!("cargo:rerun-if-changed={}", hlsl_path.display());

        if !hlsl_path.exists() {
            println!(
                "cargo:warning=Shader source not found: {}",
                hlsl_path.display()
            );
            continue;
        }

        // Compile the shader
        println!("Compiling shader: {}", shader_name);

        let output = Command::new(&fxc_path)
            .args([
                "/T", "ps_4_0", // Pixel shader model 4.0
                "/E", "main", // Entry point
                "/Fo",  // Output file
            ])
            .arg(&cso_path)
            .arg(&hlsl_path)
            .output()
            .expect("Failed to execute fxc.exe");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            panic!(
                "Failed to compile shader {}:\nstdout: {}\nstderr: {}",
                shader_name, stdout, stderr
            );
        }

        println!("Successfully compiled: {}.cso", shader_name);
    }
}

/// Finds the fxc.exe shader compiler.
///
/// Search order:
/// 1. FXC_PATH environment variable
/// 2. Windows SDK installation directories
fn find_fxc() -> Option<PathBuf> {
    // Check environment variable first
    if let Ok(path) = env::var("FXC_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    // Search in Windows SDK directories
    let program_files_x86 =
        env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());

    let sdk_base = Path::new(&program_files_x86).join("Windows Kits\\10\\bin");

    if !sdk_base.exists() {
        return None;
    }

    // Find the latest SDK version
    let mut versions: Vec<_> = std::fs::read_dir(&sdk_base)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            // SDK versions start with "10."
            if name.starts_with("10.") {
                Some((name, entry.path()))
            } else {
                None
            }
        })
        .collect();

    // Sort by version number (descending) to get the latest
    versions.sort_by(|a, b| b.0.cmp(&a.0));

    // Try to find fxc.exe in x64 directory first, then x86
    for (_version, sdk_path) in versions {
        for arch in ["x64", "x86"] {
            let fxc_path = sdk_path.join(arch).join("fxc.exe");
            if fxc_path.exists() {
                return Some(fxc_path);
            }
        }
    }

    None
}
