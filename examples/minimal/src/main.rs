include!(concat!(env!("OUT_DIR"), "/config.rs"));

fn main() {
    if CONFIG_FOO {
        let buf = [0u8; CONFIG_BUFFER_SIZE as usize];
        println!("Board: {}", CONFIG_BOARD_NAME);
        println!("Buffer length: {}", buf.len());
    }

    #[cfg(CONFIG_FEATURE)]
    println!("CONFIG_FEATURE is enabled via rustc-cfg");

    #[cfg(not(CONFIG_FEATURE))]
    println!("CONFIG_FEATURE is disabled");

    if CONFIG_BOARD_EXTRA {
        println!("board extra is on");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_constants_have_expected_types() {
        let _enabled: bool = CONFIG_FOO;
        let _size: u32 = CONFIG_BUFFER_SIZE;
        let _name: &'static str = CONFIG_BOARD_NAME;
        let _buf = [0u8; CONFIG_BUFFER_SIZE as usize];
        assert!(CONFIG_FOO);
        assert_eq!(CONFIG_BUFFER_SIZE, 256);
        assert_eq!(CONFIG_BOARD_NAME, "qemu-virt");
    }
}
