use cargo_kconfig::Error;
use cargo_kconfig::domain::{IssueKind, Tristate, Value};
use cargo_kconfig::{GenerateRequest, Pipeline};
use std::path::PathBuf;
use std::process::Command;

fn example_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

fn evaluate(example: &str, defconfig: &str) -> cargo_kconfig::GenerateResult {
    let dir = example_dir(example);
    Pipeline::new()
        .run(&GenerateRequest::new(
            dir.clone(),
            dir.join("Kconfig"),
            dir.join(defconfig),
        ))
        .unwrap_or_else(|err| panic!("{example}/{defconfig}: {err}"))
}

fn expect_error(
    example: &str,
    defconfig: &str,
    kind: IssueKind,
    needles: &[&str],
) -> cargo_kconfig::ValidationReport {
    let dir = example_dir(example);
    let err = match Pipeline::new().run(&GenerateRequest::new(
        dir.clone(),
        dir.join("Kconfig"),
        dir.join(defconfig),
    )) {
        Err(err) => err,
        Ok(_) => panic!("{example}/{defconfig} succeeded, expected {kind:?}"),
    };
    let Error::Validation(report) = err else {
        panic!("{example}/{defconfig}: expected validation error, got {err}");
    };
    assert!(
        report.has_kind(kind),
        "{example}/{defconfig} missing {kind:?}: {report}"
    );
    let text = report.to_string();
    for needle in needles {
        assert!(
            text.contains(needle),
            "{example}/{defconfig} missing `{needle}` in:\n{text}"
        );
    }
    report
}

#[test]
fn depends_bus_defconfig_satisfies_and_or_and_if() {
    let result = evaluate("depends", "bus_defconfig");
    assert_eq!(result.evaluated.get("BUS"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("UART"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("UART_DMA"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("RTT"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("CONSOLE"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("POLL_LOOP"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("UART_BAUD"), Some(&Value::Int(115200)));
    assert!(
        result
            .generated
            .rustc_cfgs
            .contains(&"CONFIG_UART".to_string())
    );
    assert!(
        !result
            .generated
            .rustc_cfgs
            .contains(&"CONFIG_POLL_LOOP".to_string())
    );
}

#[test]
fn depends_no_bus_defconfig_uses_negation_and_or() {
    let result = evaluate("depends", "no_bus_defconfig");
    assert_eq!(result.evaluated.get("BUS"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("UART"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("UART_DMA"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("RTT"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("CONSOLE"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("POLL_LOOP"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("UART_BAUD"), Some(&Value::Int(0)));
}

#[test]
fn firmware_debug_defconfig_enables_sourced_network_options() {
    let result = evaluate("firmware", "debug_defconfig");
    assert_eq!(result.evaluated.get("HAS_FPU"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_NET"), Some(&Value::Bool(true)));
    assert_eq!(
        result.evaluated.get("FLASH_BASE"),
        Some(&Value::Hex(0x800_0000))
    );
    assert_eq!(result.evaluated.get("LOG_LEVEL"), Some(&Value::Int(5)));
    assert_eq!(
        result.evaluated.get("NET_DRIVER"),
        Some(&Value::Tristate(Tristate::Yes))
    );
    assert_eq!(result.evaluated.get("TCP"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("TLS"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("NET_BUFFER"), Some(&Value::Int(4096)));
    assert_eq!(result.evaluated.get("DHCP"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("MDNS"), Some(&Value::Bool(true)));
    assert_eq!(
        result.evaluated.get("BARE_METAL_HOOKS"),
        Some(&Value::Bool(false))
    );
    assert!(result.evaluated.table.contains("DHCP"));
    assert!(
        result
            .loaded_files
            .iter()
            .any(|p| p.to_string_lossy().contains("Kconfig.net"))
    );
}

#[test]
fn firmware_prod_defconfig_clears_unmet_network_children() {
    let result = evaluate("firmware", "prod_defconfig");
    assert_eq!(result.evaluated.get("NETWORK"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("HAS_NET"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("LOGGING"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("LOG_LEVEL"), Some(&Value::Int(0)));
    assert_eq!(
        result.evaluated.get("NET_DRIVER"),
        Some(&Value::Tristate(Tristate::No))
    );
    assert_eq!(result.evaluated.get("TCP"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("TLS"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("NET_BUFFER"), Some(&Value::Int(0)));
    assert_eq!(result.evaluated.get("DHCP"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("MDNS"), Some(&Value::Bool(false)));
    assert_eq!(
        result.evaluated.get("BARE_METAL_HOOKS"),
        Some(&Value::Bool(true))
    );
    assert_eq!(result.evaluated.get("SENSOR"), Some(&Value::Bool(true)));
}

fn cargo_test_example(name: &str, defconfig: &str) {
    let dir = example_dir(name);
    let target = tempfile::tempdir().expect("target dir");
    let output = Command::new("cargo")
        .arg("test")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target.path())
        .arg("--")
        .arg("--nocapture")
        .env("KCONFIG_DEFCONFIG", defconfig)
        .output()
        .expect("cargo test");
    assert!(
        output.status.success(),
        "cargo test {name} ({defconfig}) failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn depends_example_builds_for_both_defconfigs() {
    cargo_test_example("depends", "bus_defconfig");
    cargo_test_example("depends", "no_bus_defconfig");
}

#[test]
fn firmware_example_builds_for_debug_and_prod() {
    cargo_test_example("firmware", "debug_defconfig");
    cargo_test_example("firmware", "prod_defconfig");
}

#[test]
fn select_uart_defconfig_turns_on_has_uart() {
    let result = evaluate("select", "uart_defconfig");
    assert_eq!(result.evaluated.get("UART_FOO"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_UART"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_DMA"), Some(&Value::Bool(false)));
    assert!(result.evaluated.warnings.is_empty());
}

#[test]
fn select_dma_defconfig_turns_on_has_dma() {
    let result = evaluate("select", "dma_defconfig");
    assert_eq!(result.evaluated.get("DMA_DRIVER"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("BUS"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_DMA"), Some(&Value::Bool(true)));
}

#[test]
fn select_unmet_defconfig_is_rejected() {
    let dir = example_dir("select");
    let err = Pipeline::new()
        .run(&GenerateRequest::new(
            dir.clone(),
            dir.join("Kconfig"),
            dir.join("unmet_defconfig"),
        ))
        .expect_err("DMA_DRIVER without BUS must fail");
    let Error::Validation(report) = err else {
        panic!("expected validation error, got {err}");
    };
    assert!(report.has_kind(IssueKind::UnmetDependency), "{report}");
    assert!(report.to_string().contains("HAS_DMA"), "{report}");
}

#[test]
fn select_imply_defconfig_turns_on_logging() {
    let result = evaluate("select", "imply_defconfig");
    assert_eq!(
        result.evaluated.get("DEBUG_BUILD"),
        Some(&Value::Bool(true))
    );
    assert_eq!(result.evaluated.get("LOGGING"), Some(&Value::Bool(true)));
    assert!(result.evaluated.warnings.is_empty());
}

#[test]
fn select_example_builds_for_valid_defconfigs() {
    cargo_test_example("select", "uart_defconfig");
    cargo_test_example("select", "dma_defconfig");
    cargo_test_example("select", "imply_defconfig");
}

#[test]
fn choice_uart_defconfig_is_exclusive() {
    let result = evaluate("choice", "uart_defconfig");
    assert_eq!(
        result.evaluated.get("UART_CONSOLE"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        result.evaluated.get("RTT_CONSOLE"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        result.evaluated.get("USB_CONSOLE"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        result.evaluated.get("CONSOLE_BAUD"),
        Some(&Value::Int(115200))
    );
}

#[test]
fn choice_rtt_defconfig_is_exclusive() {
    let result = evaluate("choice", "rtt_defconfig");
    assert_eq!(
        result.evaluated.get("UART_CONSOLE"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        result.evaluated.get("RTT_CONSOLE"),
        Some(&Value::Bool(true))
    );
    assert_eq!(result.evaluated.get("CONSOLE_BAUD"), Some(&Value::Int(0)));
}

#[test]
fn choice_example_builds_for_both_defconfigs() {
    cargo_test_example("choice", "uart_defconfig");
    cargo_test_example("choice", "rtt_defconfig");
}

#[test]
fn depends_unmet_defconfig_is_rejected() {
    expect_error(
        "depends",
        "unmet_defconfig",
        IssueKind::UnmetDependency,
        &["CONFIG_UART", "CONFIG_BUS"],
    );
}

#[test]
fn choice_conflict_defconfig_is_rejected() {
    expect_error(
        "choice",
        "conflict_defconfig",
        IssueKind::ChoiceConflict,
        &["CONFIG_UART_CONSOLE", "CONFIG_RTT_CONSOLE", "n"],
    );
}

#[test]
fn errors_ok_defconfig_enables_select_and_imply_helpers() {
    let result = evaluate("errors", "ok_defconfig");
    assert_eq!(result.evaluated.get("BUS"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("UART"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("UART_IRQ"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_DMA"), Some(&Value::Bool(true)));
    assert_eq!(result.evaluated.get("HAS_RTC"), Some(&Value::Bool(true)));
    assert_eq!(
        result.evaluated.get("UART_CONSOLE"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        result.evaluated.get("RTT_CONSOLE"),
        Some(&Value::Bool(false))
    );
    assert_eq!(result.evaluated.get("BUF_SIZE"), Some(&Value::Int(16)));
}

#[test]
fn errors_imply_without_bus_leaves_target_off() {
    let result = evaluate("errors", "imply_respected_defconfig");
    assert_eq!(
        result.evaluated.get("BOARD_WANTS_RTC"),
        Some(&Value::Bool(true))
    );
    assert_eq!(result.evaluated.get("BUS"), Some(&Value::Bool(false)));
    assert_eq!(result.evaluated.get("HAS_RTC"), Some(&Value::Bool(false)));
}

#[test]
fn errors_depends_on_keyword_is_rejected() {
    expect_error(
        "errors",
        "depends_unmet_defconfig",
        IssueKind::UnmetDependency,
        &["CONFIG_UART", "CONFIG_BUS"],
    );
}

#[test]
fn errors_select_keyword_is_rejected() {
    expect_error(
        "errors",
        "select_unmet_defconfig",
        IssueKind::UnmetDependency,
        &["CONFIG_HAS_DMA", "CONFIG_DMA_DRIVER", "CONFIG_BUS"],
    );
}

#[test]
fn errors_imply_keyword_user_force_is_rejected() {
    expect_error(
        "errors",
        "imply_unmet_defconfig",
        IssueKind::UnmetDependency,
        &["CONFIG_HAS_RTC"],
    );
}

#[test]
fn errors_if_keyword_is_rejected() {
    expect_error(
        "errors",
        "if_unmet_defconfig",
        IssueKind::UnmetDependency,
        &["CONFIG_UART_IRQ"],
    );
}

#[test]
fn errors_choice_keyword_is_rejected() {
    expect_error(
        "errors",
        "choice_conflict_defconfig",
        IssueKind::ChoiceConflict,
        &["CONFIG_UART_CONSOLE", "CONFIG_RTT_CONSOLE", "n"],
    );
}

#[test]
fn errors_range_keyword_is_rejected() {
    expect_error(
        "errors",
        "range_bad_defconfig",
        IssueKind::OutOfRange,
        &["CONFIG_BUF_SIZE"],
    );
}

#[test]
fn errors_example_builds_for_valid_defconfigs() {
    cargo_test_example("errors", "ok_defconfig");
    cargo_test_example("errors", "imply_respected_defconfig");
}

#[test]
fn workspace_lib_builds_from_member_and_tests_bad_defconfigs() {
    cargo_test_example("workspace/lib", "configs/qemu_defconfig");
}

#[test]
fn workspace_app_uses_lib_config_module() {
    let dir = example_dir("workspace/app");
    let target = tempfile::tempdir().expect("target dir");
    let output = Command::new("cargo")
        .arg("run")
        .arg("--offline")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target.path())
        .output()
        .expect("cargo run workspace app");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "workspace app failed:\n{stdout}\n{stderr}"
    );
    assert!(stdout.contains("workspace-virt"), "{stdout}");
    assert!(stdout.contains("UART is on"), "{stdout}");
}
