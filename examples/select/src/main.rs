pub mod config {
    cargo_kcfg_macros::include_config!();
}

use config::*;

fn main() {
    println!("HAS_UART     = {CONFIG_HAS_UART}  (selected by UART_FOO)");
    println!("UART_FOO     = {CONFIG_UART_FOO}");
    println!("UART_BAR     = {CONFIG_UART_BAR}  (select HAS_UART if HAS_PINMUX)");
    println!("HAS_PINMUX   = {CONFIG_HAS_PINMUX}");
    println!("BUS          = {CONFIG_BUS}");
    println!("HAS_DMA      = {CONFIG_HAS_DMA}  (depends on BUS; selected by DMA_DRIVER)");
    println!("DMA_DRIVER   = {CONFIG_DMA_DRIVER}");
    println!("LOGGING      = {CONFIG_LOGGING}  (implied by DEBUG_BUILD)");
    println!("DEBUG_BUILD  = {CONFIG_DEBUG_BUILD}");

    if CONFIG_HAS_UART {
        println!("compiled with UART helper (turned on by select)");
    }

    if CONFIG_HAS_DMA {
        println!("compiled with DMA helper (selected by DMA_DRIVER, requires BUS)");
    }

    if CONFIG_LOGGING {
        println!("compiled with logging (implied by DEBUG_BUILD)");
    }
}

#[cfg(test)]
mod tests {
    use super::config::*;

    #[test]
    fn uart_foo_selects_has_uart() {
        if !CONFIG_UART_FOO {
            return;
        }
        assert!(CONFIG_UART_FOO);
        assert!(CONFIG_HAS_UART);
        assert!(!CONFIG_UART_BAR);
        assert!(!CONFIG_DMA_DRIVER);
        assert!(!CONFIG_HAS_DMA);
        assert!(!CONFIG_DEBUG_BUILD);
        assert!(!CONFIG_LOGGING);
    }

    #[test]
    fn dma_driver_selects_has_dma_when_bus_is_on() {
        if !CONFIG_DMA_DRIVER {
            return;
        }
        assert!(CONFIG_DMA_DRIVER);
        assert!(CONFIG_BUS);
        assert!(CONFIG_HAS_DMA);
        assert!(!CONFIG_UART_FOO);
        assert!(!CONFIG_HAS_UART);
    }

    #[test]
    fn debug_build_implies_logging() {
        if !CONFIG_DEBUG_BUILD {
            return;
        }
        assert!(CONFIG_DEBUG_BUILD);
        assert!(CONFIG_LOGGING);
        assert!(!CONFIG_UART_FOO);
        assert!(!CONFIG_DMA_DRIVER);
    }
}
