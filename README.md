# cargo-kconfig

A Cargo subcommand that brings Linux/Zephyr-style **Kconfig** support to Rust
projects.

It parses Kconfig definition files (via [`nom-kconfig`](https://crates.io/crates/nom-kconfig)),
loads user assignments from `*_defconfig` files, evaluates dependencies,
defaults, reverse dependencies (`select` / `imply`), and `choice` exclusivity,
validates the result, and emits:

- a human-readable `.config`
- a generated `config.rs` with typed `const` items (`bool`, `u8`/`u16`/`u32`/`u64` or signed widths, `&'static str`, `Tristate`)
- `cargo:rustc-cfg=...` flags so `#[cfg]` / `cfg!` work for enabled boolean options

## Commands

```bash
cargo kconfig check
cargo kconfig build
cargo kconfig test
```

`check` validates only. `build` writes `.config` and `config.rs`. `test`
additionally type-checks the generated constants with `rustc`.

Useful flags:

```text
--root <DIR>           Project root used to resolve `source` paths
--kconfig <FILE>       Root Kconfig (default: <root>/Kconfig)
--defconfig <FILE>     Assignment file (`*_defconfig` or `.config`)
--out-dir <DIR>        Where to write generated files
--emit-rustc-cfg       Print cargo rustc-cfg lines (for build.rs)
```

## Using generated constants

```rust
if CONFIG_FOO {
    let buf = [0u8; CONFIG_BUFFER_SIZE as usize];
    println!("Board: {}", CONFIG_BOARD_NAME);
}

#[cfg(CONFIG_FEATURE)]
fn feature_only() {}
```

From a `build.rs`:

```rust
fn main() {
    if cargo_kconfig::run_build_script().is_err() {
        std::process::exit(1);
    }
}
```

That helper finds `Kconfig` and a single `*_defconfig` (or `KCONFIG_DEFCONFIG`), writes `config.rs` / `.config` into `OUT_DIR`, and prints `cargo:rustc-cfg` lines. Several `*_defconfig` files are an error unless you name one.

See the buildable examples:

| Example | What it shows |
| --- | --- |
| `examples/minimal` | Bool / int / string constants, `source`, and `#[cfg]` |
| `examples/depends` | `depends on`, `&&` / `||` / `!`, and `if` blocks. Switch with `KCONFIG_DEFCONFIG=no_bus_defconfig` |
| `examples/firmware` | Menus, `default ... if`, `range`, `tristate`, hex, `def_bool`, `select HAS_NET`, and a sourced file gated by `if NETWORK` |
| `examples/select` | `select` auto-enables helpers; enabling a target whose `depends on` is unmet is an error. `imply` is a weak default. Switch with `KCONFIG_DEFCONFIG=dma_defconfig` or `imply_defconfig` |
| `examples/choice` | Bool `choice` exclusivity. Switch with `KCONFIG_DEFCONFIG=rtt_defconfig` |
| `examples/errors` | Each keyword that must **fail**: `depends on`, `select`, `imply` (user `y`), `if`, `choice`, `range`. Valid defaults: `ok_defconfig` and `imply_respected_defconfig` |

```bash
KCONFIG_DEFCONFIG=bus_defconfig cargo run --manifest-path examples/depends/Cargo.toml
KCONFIG_DEFCONFIG=no_bus_defconfig cargo run --manifest-path examples/depends/Cargo.toml

KCONFIG_DEFCONFIG=debug_defconfig cargo run --manifest-path examples/firmware/Cargo.toml
KCONFIG_DEFCONFIG=prod_defconfig cargo run --manifest-path examples/firmware/Cargo.toml

KCONFIG_DEFCONFIG=uart_defconfig cargo run --manifest-path examples/select/Cargo.toml
KCONFIG_DEFCONFIG=dma_defconfig cargo run --manifest-path examples/select/Cargo.toml
KCONFIG_DEFCONFIG=imply_defconfig cargo run --manifest-path examples/select/Cargo.toml

KCONFIG_DEFCONFIG=uart_defconfig cargo run --manifest-path examples/choice/Cargo.toml
KCONFIG_DEFCONFIG=rtt_defconfig cargo run --manifest-path examples/choice/Cargo.toml

KCONFIG_DEFCONFIG=ok_defconfig cargo run --manifest-path examples/errors/Cargo.toml
KCONFIG_DEFCONFIG=imply_respected_defconfig cargo run --manifest-path examples/errors/Cargo.toml
```

Invalid defconfigs error out instead of generating constants. Check them with the CLI:

```bash
cargo run --quiet -- check --root examples/errors --kconfig examples/errors/Kconfig --defconfig examples/errors/depends_unmet_defconfig
cargo run --quiet -- check --root examples/errors --kconfig examples/errors/Kconfig --defconfig examples/errors/select_unmet_defconfig
cargo run --quiet -- check --root examples/errors --kconfig examples/errors/Kconfig --defconfig examples/errors/imply_unmet_defconfig
cargo run --quiet -- check --root examples/errors --kconfig examples/errors/Kconfig --defconfig examples/errors/if_unmet_defconfig
cargo run --quiet -- check --root examples/errors --kconfig examples/errors/Kconfig --defconfig examples/errors/choice_conflict_defconfig
cargo run --quiet -- check --root examples/errors --kconfig examples/errors/Kconfig --defconfig examples/errors/range_bad_defconfig
```

`select` is a reverse dependency: enabling `UART_FOO` turns `HAS_UART` on
even if the user never assigned it. If the target's own `depends on` is
still unmet after that, evaluation **fails** and tells you to enable the
missing dependencies or remove the `select`. `imply` is the weaker form —
the user can keep the target at `n`, and unmet target dependencies are
respected (the imply does not fire).

## Design

The crate follows a strict domain / infrastructure split:

- **domain** — `Symbol`, `SymbolTable`, `Expression`, `Tristate`, `Value`,
  `AssignmentSet`, `ChoiceGroup`, `DependencyGraph`, evaluation, and
  `ValidationReport`. No filesystem or Cargo types.
- **infrastructure** — Kconfig loading (following `source` directives, never
  crawling the tree), defconfig parsing, code generation, CLI.

Inputs are size-limited. Unexpected types, out-of-range integers, unknown
symbols, `select`/`imply` of a non-bool target, unmet user assignments, and
`select` of a target whose `depends on` is unmet are reported as errors
rather than ignored.
