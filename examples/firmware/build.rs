use cargo_kconfig::{run, write_outputs, GenerateRequest};
use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let defconfig_name =
        env::var("KCONFIG_DEFCONFIG").unwrap_or_else(|_| "debug_defconfig".to_string());
    let request = GenerateRequest::new(
        manifest_dir.clone(),
        manifest_dir.join("Kconfig"),
        manifest_dir.join(&defconfig_name),
    );
    let result = run(&request).unwrap_or_else(|err| {
        panic!("kconfig evaluation failed using {defconfig_name}: {err}");
    });
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
    println!("cargo:rerun-if-env-changed=KCONFIG_DEFCONFIG");
    println!("cargo:rerun-if-changed=Kconfig");
    println!("cargo:rerun-if-changed=Kconfig.net");
    println!("cargo:rerun-if-changed=debug_defconfig");
    println!("cargo:rerun-if-changed=prod_defconfig");
}
