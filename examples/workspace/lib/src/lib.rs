pub mod config {
    cargo_kcfg_macros::include_config!();
}

pub fn board_name() -> &'static str {
    config::CONFIG_BOARD_NAME
}

#[cfg(test)]
mod tests {
    use super::config::*;
    use cargo_kcfg::domain::{IssueKind, Value};
    use cargo_kcfg::{ConfigTest, Error};

    #[test]
    fn generated_constants_live_in_the_config_module() {
        let _size: u16 = CONFIG_BUFFER_SIZE;
        assert!(CONFIG_FOO);
        assert_eq!(CONFIG_BUFFER_SIZE, 256);
        assert_eq!(CONFIG_BOARD_NAME, "workspace-virt");
        assert!(CONFIG_BUS);
        assert!(CONFIG_UART);
        assert_eq!(super::board_name(), "workspace-virt");
    }

    #[test]
    fn good_defconfig_takes_the_uart_on_path() {
        let probe = ConfigTest::discover().expect("workspace Kconfig");
        let cfg = probe.evaluate("configs/qemu_defconfig").unwrap();
        if cfg.get("UART") == Some(&Value::Bool(true)) {
            assert_eq!(cfg.get("BUS"), Some(&Value::Bool(true)));
        } else {
            panic!("qemu_defconfig should enable UART");
        }
    }

    #[test]
    fn bad_defconfig_takes_the_validation_error_path() {
        let probe = ConfigTest::discover().expect("workspace Kconfig");
        match probe.evaluate("configs/cases/unmet_defconfig") {
            Err(Error::Validation(report)) => {
                assert!(report.has_kind(IssueKind::UnmetDependency), "{report}");
            }
            Ok(_) => panic!("UART without BUS must fail"),
            Err(err) => panic!("{err}"),
        }
    }

    #[test]
    fn inline_bad_text_controls_test_flow() {
        let probe = ConfigTest::discover().expect("workspace Kconfig");
        match probe.evaluate_text("CONFIG_UART=y\n") {
            Err(Error::Validation(report)) if report.has_kind(IssueKind::UnmetDependency) => {}
            other => panic!("expected unmet-dependency, got {other:?}"),
        }
    }
}
