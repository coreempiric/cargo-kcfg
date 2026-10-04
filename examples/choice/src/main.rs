pub mod config {
    cargo_kcfg_macros::include_config!();
}

use config::*;

fn main() {
    println!("UART console = {CONFIG_UART_CONSOLE}");
    println!("RTT console  = {CONFIG_RTT_CONSOLE}");
    println!("USB console  = {CONFIG_USB_CONSOLE}");
    println!("UART baud    = {CONFIG_CONSOLE_BAUD}  (depends on UART_CONSOLE)");

    if CONFIG_UART_CONSOLE {
        println!("compiled with the UART console");
    }

    if CONFIG_RTT_CONSOLE {
        println!("compiled with the RTT console");
    }

    if CONFIG_USB_CONSOLE {
        println!("compiled with the USB console");
    }
}

#[cfg(test)]
mod tests {
    use super::config::*;

    #[test]
    fn uart_choice_is_exclusive() {
        if !CONFIG_UART_CONSOLE {
            return;
        }
        assert!(CONFIG_UART_CONSOLE);
        assert!(!CONFIG_RTT_CONSOLE);
        assert!(!CONFIG_USB_CONSOLE);
        assert_eq!(CONFIG_CONSOLE_BAUD, 115200);
        let _baud: u32 = CONFIG_CONSOLE_BAUD;
    }

    #[test]
    fn rtt_choice_is_exclusive() {
        if !CONFIG_RTT_CONSOLE {
            return;
        }
        assert!(!CONFIG_UART_CONSOLE);
        assert!(CONFIG_RTT_CONSOLE);
        assert!(!CONFIG_USB_CONSOLE);
        assert_eq!(CONFIG_CONSOLE_BAUD, 0);
    }
}
