use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../remote/src");
    println!("cargo:rerun-if-changed=../remote/Cargo.toml");
    
    // デフォルトのターゲットを設定（環境変数未指定時はホストターゲット）
    let target = env::var("VSCFREEDEV_REMOTE_TARGET")
        .unwrap_or_else(|_| {
            // rustcからホストターゲットを取得
            let output = Command::new("rustc")
                .arg("-vV")
                .output()
                .expect("Failed to run rustc");
            
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.starts_with("host:") {
                    return line.split_whitespace().nth(1).unwrap().to_string();
                }
            }
            
            // フォールバック
            "x86_64-unknown-linux-gnu".to_string()
        });
    
    // zigbuildが利用可能かチェック
    let use_zigbuild = Command::new("cargo")
        .arg("zigbuild")
        .arg("--version")
        .output()
        .is_ok();
    
    let mut cmd = Command::new("cargo");
    
    if use_zigbuild {
        cmd.arg("zigbuild");
    } else {
        cmd.arg("build");
    }
    
    cmd.arg("--release")
        .arg("-p")
        .arg("vscfreedev-remote")
        .arg("--target")
        .arg(&target);
    
    let output = cmd.output()
        .expect("Failed to build remote binary");
    
    if !output.status.success() {
        panic!(
            "Failed to build remote binary:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    
    // ビルドされたバイナリのパスを環境変数として設定
    let remote_binary_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .join("target")
        .join(&target)
        .join("release")
        .join("vscfreedev-remote");
    
    println!(
        "cargo:rustc-env=VSCFREEDEV_REMOTE_BINARY_PATH={}",
        remote_binary_path.display()
    );
}