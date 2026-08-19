use cargo_kconfig::{run, write_outputs, GenerateRequest};
use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let request = GenerateRequest::new(
        manifest_dir.clone(),
        manifest_dir.join("Kconfig"),
        manifest_dir.join("qemu_defconfig"),
    );
    let result = run(&request).expect("kconfig evaluation failed");
    for warning in &result.evaluated.warnings {
        println!("cargo:warning={}", warning.message);
    }
    write_outputs(
        &result,
        &out_dir.join("config.rs"),
        &out_dir.join(".config"),
    )
    .expect("write generated files");

    for cfg in &result.generated.rustc_check_cfgs {
        println!("cargo:rustc-check-cfg={cfg}");
    }
    for cfg in &result.generated.rustc_cfgs {
        println!("cargo:rustc-cfg={cfg}");
    }
    println!("cargo:rerun-if-changed=Kconfig");
    println!("cargo:rerun-if-changed=Kconfig.board");
    println!("cargo:rerun-if-changed=qemu_defconfig");
}
