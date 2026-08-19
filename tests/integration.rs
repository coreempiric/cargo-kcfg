use cargo_kconfig::Error;
use cargo_kconfig::domain::{IssueKind, Value};
use cargo_kconfig::infra::pipeline::{GenerateRequest, run};
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn request(dir: &Path, defconfig: &str) -> GenerateRequest {
    GenerateRequest::new(dir.to_path_buf(), dir.join("Kconfig"), dir.join(defconfig))
}

fn expect_validation(
    result: Result<cargo_kconfig::GenerateResult, Error>,
) -> cargo_kconfig::ValidationReport {
    match result {
        Err(Error::Validation(report)) => report,
        other => panic!("expected validation error, got {other:?}"),
    }
}

#[test]
fn sourced_tree_produces_dotconfig_and_typed_constants() {
    let dir = fixture("happy_sourced");
    let result = run(&request(&dir, "qemu_defconfig")).expect("pipeline");

    assert_eq!(result.evaluated.get("FOO"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("BUFFER_SIZE"), Some(&Value::Int(256)));
    assert_eq!(
        result.evaluated.get("BOARD_NAME"),
        Some(&Value::String("qemu-virt".into()))
    );
    assert_eq!(result.evaluated.get("FEATURE"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("DMA_ADDR"), Some(&Value::Hex(0x2000)));
    assert_eq!(
        result.evaluated.get("BOARD_EXTRA"),
        Some(&Value::Bool(true))
    );

    let dot = &result.generated.dotconfig;
    assert!(dot.contains("CONFIG_FOO=y"), "{dot}");
    assert!(dot.contains("CONFIG_BUFFER_SIZE=256"), "{dot}");
    assert!(dot.contains("CONFIG_BOARD_NAME=\"qemu-virt\""), "{dot}");
    assert!(dot.contains("CONFIG_DMA_ADDR=0x2000"), "{dot}");
    assert!(dot.contains("CONFIG_BOARD_EXTRA=y"), "{dot}");

    let rs = &result.generated.config_rs;
    assert!(rs.contains("pub const CONFIG_FOO: bool = true;"), "{rs}");
    assert!(
        rs.contains("pub const CONFIG_BUFFER_SIZE: u16 = 256;"),
        "{rs}"
    );
    assert!(
        rs.contains("pub const CONFIG_BOARD_NAME: &'static str = \"qemu-virt\";"),
        "{rs}"
    );
    assert!(
        rs.contains("pub const CONFIG_DMA_ADDR: u16 = 0x2000;"),
        "{rs}"
    );
    assert!(
        result
            .generated
            .rustc_cfgs
            .contains(&"CONFIG_FOO".to_string())
    );
    assert!(
        result
            .generated
            .rustc_cfgs
            .contains(&"CONFIG_FEATURE".to_string())
    );
}

#[test]
fn source_follows_kconfig_graph_not_directory_crawl() {
    let dir = fixture("happy_sourced");
    let result = run(&request(&dir, "qemu_defconfig")).expect("pipeline");

    assert!(result.evaluated.table.contains("BOARD_EXTRA"));
    assert!(
        !result.evaluated.table.contains("DECOY_SYMBOL"),
        "DECOY_SYMBOL was loaded; the walker crawled the filesystem instead of following source"
    );

    let loaded: Vec<String> = result
        .loaded_files
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    assert!(
        loaded
            .iter()
            .any(|p| p.ends_with("Kconfig") || p == "Kconfig"),
        "root Kconfig missing from {loaded:?}"
    );
    assert!(
        loaded.iter().any(|p| p.contains("Kconfig.board")),
        "sourced Kconfig.board missing from {loaded:?}"
    );
    assert!(
        loaded.iter().all(|p| !p.contains("decoy")),
        "decoy file was visited: {loaded:?}"
    );
}

#[test]
fn generated_constants_are_usable_from_rust() {
    let dir = fixture("happy_sourced");
    let result = run(&request(&dir, "qemu_defconfig")).expect("pipeline");
    let body = r#"
        if CONFIG_FOO {
            let buf = [0u8; CONFIG_BUFFER_SIZE as usize];
            assert_eq!(buf.len(), 256);
            let _ = CONFIG_BOARD_NAME;
            assert_eq!(CONFIG_BOARD_NAME, "qemu-virt");
            assert_eq!(CONFIG_DMA_ADDR, 0x2000);
        } else {
            panic!("CONFIG_FOO should be enabled");
        }
    "#;
    compile_with_generated(&result.generated.config_rs, body);
}

#[test]
fn rejects_unknown_symbols() {
    let dir = fixture("unknown_symbol");
    let report = expect_validation(run(&request(&dir, "bad_defconfig")));
    assert!(report.has_kind(IssueKind::UnknownSymbol), "{report}");
    assert!(report.to_string().contains("DOES_NOT_EXIST"), "{report}");
}

#[test]
fn rejects_type_incorrect_values() {
    let dir = fixture("type_mismatch");
    let report = expect_validation(run(&request(&dir, "bad_defconfig")));
    assert!(report.has_kind(IssueKind::TypeMismatch), "{report}");
}

#[test]
fn rejects_out_of_range_integers() {
    let dir = fixture("out_of_range");
    let report = expect_validation(run(&request(&dir, "bad_defconfig")));
    assert!(report.has_kind(IssueKind::OutOfRange), "{report}");
}

#[test]
fn rejects_unmet_dependencies() {
    let dir = fixture("depends_unmet");
    let report = expect_validation(run(&request(&dir, "bad_defconfig")));
    assert!(report.has_kind(IssueKind::UnmetDependency), "{report}");
}

#[test]
fn select_raises_helper_without_a_user_assignment() {
    let dir = fixture("select_imply");
    let result = run(&request(&dir, "uart_defconfig")).expect("pipeline");
    assert_eq!(result.evaluated.get("UART"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_UART"), Some(&Value::Bool(true)));
    assert!(result.evaluated.warnings.is_empty());
    assert!(result.generated.dotconfig.contains("CONFIG_HAS_UART=y"));
    assert!(
        result
            .generated
            .rustc_cfgs
            .contains(&"CONFIG_HAS_UART".to_string())
    );
}

#[test]
fn select_unmet_target_deps_is_rejected() {
    let dir = fixture("select_imply");
    let report = expect_validation(run(&request(&dir, "unmet_defconfig")));
    assert!(report.has_kind(IssueKind::UnmetDependency), "{report}");
    assert!(report.to_string().contains("CONFIG_HAS_DMA"), "{report}");
    assert!(
        report.to_string().contains("Set `CONFIG_BUS=y`"),
        "{report}"
    );
}

#[test]
fn imply_raises_until_the_user_sets_n() {
    let dir = fixture("select_imply");
    let on = run(&request(&dir, "imply_defconfig")).expect("pipeline");
    assert_eq!(on.evaluated.get("DEBUG"), Some(&Value::Bool(true)));
    assert_eq!(on.evaluated.get("LOG"), Some(&Value::Bool(true)));
    assert!(on.evaluated.warnings.is_empty());

    let off = run(&request(&dir, "imply_off_defconfig")).expect("pipeline");
    assert_eq!(off.evaluated.get("DEBUG"), Some(&Value::Bool(true)));
    assert_eq!(off.evaluated.get("LOG"), Some(&Value::Bool(false)));
}

#[test]
fn rejects_select_of_unknown_symbol() {
    let dir = fixture("select_unknown");
    let report = expect_validation(run(&request(&dir, "bad_defconfig")));
    assert!(report.has_kind(IssueKind::UnknownSymbol), "{report}");
    assert!(report.to_string().contains("MISSING"), "{report}");
}

#[test]
fn rejects_select_of_non_bool_symbol() {
    let dir = fixture("select_nonbool");
    let report = expect_validation(run(&request(&dir, "empty_defconfig")));
    assert!(report.has_kind(IssueKind::TypeMismatch), "{report}");
    assert!(report.to_string().contains("SIZE"), "{report}");
}

#[test]
fn choice_selects_exactly_one_member() {
    let dir = fixture("choice_ok");
    let uart = run(&request(&dir, "uart_defconfig")).expect("pipeline");
    assert_eq!(uart.evaluated.get("UART_CONSOLE"), Some(&Value::Bool(true)));
    assert_eq!(uart.evaluated.get("RTT_CONSOLE"), Some(&Value::Bool(false)));
    assert_eq!(uart.evaluated.get("USB_CONSOLE"), Some(&Value::Bool(false)));
    assert_eq!(
        uart.evaluated.get("CONSOLE_BAUD"),
        Some(&Value::Int(115200))
    );
    assert!(uart.generated.dotconfig.contains("CONFIG_UART_CONSOLE=y"));

    let rtt = run(&request(&dir, "rtt_defconfig")).expect("pipeline");
    assert_eq!(rtt.evaluated.get("UART_CONSOLE"), Some(&Value::Bool(false)));
    assert_eq!(rtt.evaluated.get("RTT_CONSOLE"), Some(&Value::Bool(true)));
    assert_eq!(rtt.evaluated.get("CONSOLE_BAUD"), Some(&Value::Int(0)));
}

#[test]
fn rejects_choice_with_two_members_enabled() {
    let dir = fixture("choice_conflict");
    let report = expect_validation(run(&request(&dir, "bad_defconfig")));
    assert!(report.has_kind(IssueKind::ChoiceConflict), "{report}");
    assert!(
        report.to_string().contains("more than one member enabled"),
        "{report}"
    );
}

#[test]
fn rejects_cyclic_direct_dependencies() {
    let dir = fixture("cyclic_depends");
    let report = expect_validation(run(&request(&dir, "empty_defconfig")));
    assert!(report.has_kind(IssueKind::CyclicDependency), "{report}");
}

#[test]
fn applies_defaults_when_defconfig_is_empty() {
    let dir = fixture("defaults_only");
    let result = run(&request(&dir, "empty_defconfig")).expect("pipeline");
    assert_eq!(result.evaluated.get("FOO"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("SIZE"), Some(&Value::Int(32)));
    assert!(result.generated.dotconfig.contains("CONFIG_FOO=y"));
    assert!(result.generated.dotconfig.contains("CONFIG_SIZE=32"));
}

fn compile_with_generated(config_rs: &str, body: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut source = config_rs.to_string();
    source.push_str("\nfn main() {\n");
    source.push_str(body);
    source.push_str("\n}\n");
    let src_path = dir.path().join("main.rs");
    std::fs::write(&src_path, source).unwrap();
    let output = Command::new("rustc")
        .arg("--edition")
        .arg("2024")
        .arg("-o")
        .arg(dir.path().join("main"))
        .arg(&src_path)
        .output()
        .expect("rustc");
    assert!(
        output.status.success(),
        "rustc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(dir.path().join("main")).output().expect("run");
    assert!(
        run.status.success(),
        "generated program failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
