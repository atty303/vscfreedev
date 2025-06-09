use std::env;
use std::path::PathBuf;
use std::process::Command;

type BuildResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Configuration for the remote binary build
struct BuildConfig {
    target: String,
    use_zigbuild: bool,
}

impl BuildConfig {
    fn new() -> BuildResult<Self> {
        let target = Self::determine_target()?;
        let use_zigbuild = Self::check_zigbuild_available();

        Ok(Self {
            target,
            use_zigbuild,
        })
    }

    fn determine_target() -> BuildResult<String> {
        // Check environment variable first
        if let Ok(target) = env::var("YUHA_REMOTE_TARGET") {
            return Ok(target);
        }

        // Fallback to host target from rustc
        Self::get_host_target()
    }

    fn get_host_target() -> BuildResult<String> {
        let output = Command::new("rustc")
            .arg("-vV")
            .output()
            .map_err(|e| format!("Failed to run rustc: {}", e))?;

        if !output.status.success() {
            return Err("rustc command failed".into());
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if let Some(target) = line.strip_prefix("host: ") {
                return Ok(target.to_string());
            }
        }

        // Fallback to common Linux target
        Ok("x86_64-unknown-linux-gnu".to_string())
    }

    fn check_zigbuild_available() -> bool {
        Command::new("cargo")
            .arg("zigbuild")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

fn build_remote_binary(config: &BuildConfig) -> BuildResult<()> {
    let mut cmd = Command::new("cargo");

    // Configure build tool
    if config.use_zigbuild {
        cmd.arg("zigbuild");
    } else {
        cmd.arg("build");
    }

    // Configure build arguments
    cmd.args(["--release", "-p", "yuha-remote", "--target", &config.target]);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute cargo command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to build remote binary:\n{}", stderr).into());
    }

    Ok(())
}

fn set_binary_path(target: &str) -> BuildResult<()> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|_| "CARGO_MANIFEST_DIR not set")?;

    let remote_binary_path = PathBuf::from(manifest_dir)
        .join("../..")
        .join("target")
        .join(target)
        .join("release")
        .join("yuha-remote");

    println!(
        "cargo:rustc-env=YUHA_REMOTE_BINARY_PATH={}",
        remote_binary_path.display()
    );

    Ok(())
}

fn main() {
    // Set up rebuild triggers
    println!("cargo:rerun-if-changed=../remote/src");
    println!("cargo:rerun-if-changed=../remote/Cargo.toml");
    println!("cargo:rerun-if-env-changed=YUHA_REMOTE_TARGET");

    // Build process
    let config = BuildConfig::new()
        .unwrap_or_else(|e| panic!("Failed to create build configuration: {}", e));

    build_remote_binary(&config).unwrap_or_else(|e| panic!("Build failed: {}", e));

    set_binary_path(&config.target).unwrap_or_else(|e| panic!("Failed to set binary path: {}", e));
}
