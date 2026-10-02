pub mod config {
    cargo_kcfg::include_config!();
}

use config::*;

fn main() {
    println!("BUS        = {CONFIG_BUS}");
    println!("UART       = {CONFIG_UART}");
    println!("UART_DMA   = {CONFIG_UART_DMA}  (depends on BUS && UART)");
    println!("RTT        = {CONFIG_RTT}");
    println!("CONSOLE    = {CONFIG_CONSOLE}  (depends on UART || RTT)");
    println!("POLL_LOOP  = {CONFIG_POLL_LOOP}  (depends on !BUS)");
    println!("UART_BAUD  = {CONFIG_UART_BAUD}  (defined inside `if UART`)");

    if CONFIG_UART {
        println!("compiled with UART (depends on BUS)");
    } else {
        println!("compiled without UART");
    }

    if CONFIG_POLL_LOOP {
        println!("compiled with the no-bus poll loop");
    }
}

#[cfg(test)]
mod tests {
    use super::config::*;

    #[test]
    fn bus_defconfig_enables_uart_graph() {
        if !CONFIG_BUS {
            return;
        }
        assert!(CONFIG_BUS);
        assert!(CONFIG_UART);
        assert!(CONFIG_UART_DMA);
        assert!(!CONFIG_RTT);
        assert!(CONFIG_CONSOLE);
        assert!(!CONFIG_POLL_LOOP);
        assert_eq!(CONFIG_UART_BAUD, 115200);
        let _baud: u32 = CONFIG_UART_BAUD;
    }

    #[test]
    fn no_bus_defconfig_uses_rtt_and_poll_loop() {
        if CONFIG_BUS {
            return;
        }
        assert!(!CONFIG_BUS);
        assert!(!CONFIG_UART);
        assert!(!CONFIG_UART_DMA);
        assert!(CONFIG_RTT);
        assert!(CONFIG_CONSOLE);
        assert!(CONFIG_POLL_LOOP);
        assert_eq!(CONFIG_UART_BAUD, 0);
    }
}
