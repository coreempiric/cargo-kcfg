pub mod config {
    cargo_kcfg::include_config!();
}

use config::*;

fn main() {
    println!("BUS              = {CONFIG_BUS}");
    println!("UART             = {CONFIG_UART}  (depends on BUS)");
    println!("UART_IRQ         = {CONFIG_UART_IRQ}  (inside `if UART`)");
    println!("DMA_DRIVER       = {CONFIG_DMA_DRIVER}  (select HAS_DMA)");
    println!("HAS_DMA          = {CONFIG_HAS_DMA}  (depends on BUS)");
    println!("BOARD_WANTS_RTC  = {CONFIG_BOARD_WANTS_RTC}  (imply HAS_RTC)");
    println!("HAS_RTC          = {CONFIG_HAS_RTC}  (depends on BUS)");
    println!("UART_CONSOLE     = {CONFIG_UART_CONSOLE}");
    println!("RTT_CONSOLE      = {CONFIG_RTT_CONSOLE}");
    println!("BUF_SIZE         = {CONFIG_BUF_SIZE}");

    if CONFIG_UART {
        println!("compiled with UART (depends on BUS)");
    }

    if CONFIG_HAS_DMA {
        println!("compiled with DMA helper (selected by DMA_DRIVER)");
    }

    if CONFIG_HAS_RTC {
        println!("compiled with RTC helper (implied by BOARD_WANTS_RTC)");
    }
}

#[cfg(test)]
mod tests {
    use super::config::*;

    #[test]
    fn ok_defconfig_satisfies_every_keyword() {
        if !CONFIG_BUS {
            return;
        }
        assert!(CONFIG_BUS);
        assert!(CONFIG_UART);
        assert!(CONFIG_UART_IRQ);
        assert!(CONFIG_DMA_DRIVER);
        assert!(CONFIG_HAS_DMA);
        assert!(CONFIG_BOARD_WANTS_RTC);
        assert!(CONFIG_HAS_RTC);
        assert!(CONFIG_UART_CONSOLE);
        assert!(!CONFIG_RTT_CONSOLE);
        assert_eq!(CONFIG_BUF_SIZE, 16);
        let _buf: u8 = CONFIG_BUF_SIZE;
    }

    #[test]
    fn imply_without_bus_leaves_rtc_off() {
        if CONFIG_BUS {
            return;
        }
        assert!(!CONFIG_BUS);
        assert!(CONFIG_BOARD_WANTS_RTC);
        assert!(!CONFIG_HAS_RTC);
        assert!(!CONFIG_UART);
        assert!(!CONFIG_HAS_DMA);
    }
}
