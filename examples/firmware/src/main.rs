pub mod config {
    cargo_kcfg::include_config!();
}

use config::*;

fn main() {
    println!("board qemu     = {CONFIG_BOARD_QEMU}");
    println!("has FPU        = {CONFIG_HAS_FPU}");
    println!("flash base     = {CONFIG_FLASH_BASE:#x}");
    println!("logging        = {CONFIG_LOGGING} (level {CONFIG_LOG_LEVEL})");
    println!("has net        = {CONFIG_HAS_NET}  (selected by NETWORK)");
    println!("network        = {CONFIG_NETWORK}");
    println!("net driver     = {:?}", CONFIG_NET_DRIVER);
    println!("tcp / udp / tls = {CONFIG_TCP} / {CONFIG_UDP} / {CONFIG_TLS}");
    println!("net buffer     = {CONFIG_NET_BUFFER}");
    println!("dhcp / mdns    = {CONFIG_DHCP} / {CONFIG_MDNS}");
    println!("spi / sensor   = {CONFIG_SPI} / {CONFIG_SENSOR}");
    println!("bare-metal     = {CONFIG_BARE_METAL_HOOKS}  (depends on !NETWORK)");

    if CONFIG_LOGGING {
        let _level: u8 = CONFIG_LOG_LEVEL;
        println!("log level is in range 0..=5: {CONFIG_LOG_LEVEL}");
    }

    if CONFIG_NETWORK && CONFIG_NET_DRIVER.is_enabled() {
        let frames = [0u8; CONFIG_NET_BUFFER as usize];
        println!("network frame buffer length: {}", frames.len());
    }

    if CONFIG_TCP {
        println!("compiled with TCP");
    }

    if CONFIG_TLS {
        println!("compiled with TLS (depends on NETWORK && TCP)");
    }

    if CONFIG_MDNS {
        println!("compiled with mDNS (sourced from Kconfig.net, depends on UDP)");
    }

    if CONFIG_BARE_METAL_HOOKS {
        println!("compiled with bare-metal hooks");
    }
}

#[cfg(test)]
mod tests {
    use super::config::*;

    #[test]
    fn shared_board_constants() {
        let _flash: u32 = CONFIG_FLASH_BASE;
        assert!(CONFIG_BOARD_QEMU);
        assert!(CONFIG_HAS_FPU);
        assert!(CONFIG_SPI);
        assert!(CONFIG_SENSOR);
        assert_eq!(CONFIG_FLASH_BASE, 0x800_0000);
    }

    #[test]
    fn debug_defconfig_enables_the_network_stack() {
        if !CONFIG_NETWORK {
            return;
        }
        assert!(CONFIG_LOGGING);
        assert_eq!(CONFIG_LOG_LEVEL, 5);
        assert!(CONFIG_NETWORK);
        assert!(CONFIG_HAS_NET);
        assert!(CONFIG_NET_DRIVER.is_enabled());
        assert_eq!(CONFIG_NET_DRIVER, Tristate::Yes);
        assert!(CONFIG_TCP);
        assert!(CONFIG_UDP);
        assert!(CONFIG_TLS);
        assert_eq!(CONFIG_NET_BUFFER, 4096);
        assert!(CONFIG_DHCP);
        assert!(CONFIG_MDNS);
        assert!(!CONFIG_BARE_METAL_HOOKS);
        let _frames = [0u8; CONFIG_NET_BUFFER as usize];
    }

    #[test]
    fn prod_defconfig_drops_network_children() {
        if CONFIG_NETWORK {
            return;
        }
        assert!(!CONFIG_LOGGING);
        assert_eq!(CONFIG_LOG_LEVEL, 0);
        assert!(!CONFIG_NETWORK);
        assert!(!CONFIG_HAS_NET);
        assert!(!CONFIG_NET_DRIVER.is_enabled());
        assert!(!CONFIG_TCP);
        assert!(!CONFIG_UDP);
        assert!(!CONFIG_TLS);
        assert_eq!(CONFIG_NET_BUFFER, 0);
        assert!(!CONFIG_DHCP);
        assert!(!CONFIG_MDNS);
        assert!(CONFIG_BARE_METAL_HOOKS);
    }
}
