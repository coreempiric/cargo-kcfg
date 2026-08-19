include!(concat!(env!("OUT_DIR"), "/config.rs"));

fn main() {
    println!("UART console = {CONFIG_UART_CONSOLE}");
    println!("RTT console  = {CONFIG_RTT_CONSOLE}");
    println!("USB console  = {CONFIG_USB_CONSOLE}");
    println!("UART baud    = {CONFIG_CONSOLE_BAUD}  (depends on UART_CONSOLE)");

    #[cfg(CONFIG_UART_CONSOLE)]
    println!("compiled with the UART console");

    #[cfg(CONFIG_RTT_CONSOLE)]
    println!("compiled with the RTT console");

    #[cfg(CONFIG_USB_CONSOLE)]
    println!("compiled with the USB console");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(CONFIG_UART_CONSOLE)]
    #[test]
    fn uart_choice_is_exclusive() {
        assert!(CONFIG_UART_CONSOLE);
        assert!(!CONFIG_RTT_CONSOLE);
        assert!(!CONFIG_USB_CONSOLE);
        assert_eq!(CONFIG_CONSOLE_BAUD, 115200);
        let _baud: u32 = CONFIG_CONSOLE_BAUD;
    }

    #[cfg(CONFIG_RTT_CONSOLE)]
    #[test]
    fn rtt_choice_is_exclusive() {
        assert!(!CONFIG_UART_CONSOLE);
        assert!(CONFIG_RTT_CONSOLE);
        assert!(!CONFIG_USB_CONSOLE);
        assert_eq!(CONFIG_CONSOLE_BAUD, 0);
    }
}
