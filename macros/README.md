# cargo-kcfg-macros

Proc-macro for [`cargo-kcfg`](https://crates.io/crates/cargo-kcfg). Expands
evaluated Kconfig constants into your crate with no `build.rs`.

```toml
[dependencies]
cargo-kcfg-macros = "0.0.1"
```

```rust,ignore
pub mod config {
    cargo_kcfg_macros::include_config!();
}

use config::*;

pub fn init() {
    if CONFIG_FOO {
        let _buf = [0u8; CONFIG_BUFFER_SIZE as usize];
    }
}
```

Place `Kconfig` and a unique `*_defconfig` (or `configs/*_defconfig`) at the
crate or workspace root. Switch products with `KCONFIG_DEFCONFIG`.

This crate is a proc-macro (always compiled for the host), so it is safe on
`no_std` targets. It cannot emit `cargo:rustc-cfg`; use `if CONFIG_FOO`.
For `#[cfg(CONFIG_FOO)]`, see `cargo_kcfg::run_build_script`.

Full guide: <https://docs.rs/cargo-kcfg>.
