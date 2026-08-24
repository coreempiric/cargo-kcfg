use kconfig_example_workspace_lib::config::*;
use kconfig_example_workspace_lib::board_name;

fn main() {
    println!("Board: {}", board_name());
    if CONFIG_FOO {
        let buf = [0u8; CONFIG_BUFFER_SIZE as usize];
        println!("Buffer length: {}", buf.len());
    }
    if CONFIG_UART {
        println!("UART is on (BUS is also on)");
    }
}
