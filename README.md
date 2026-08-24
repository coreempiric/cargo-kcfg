# cargo-kconfig

A Cargo subcommand that brings Linux/Zephyr-style **Kconfig** support to Rust
projects.

It parses Kconfig definition files (via [`nom-kconfig`](https://crates.io/crates/nom-kconfig)),
loads user assignments from `*_defconfig` files, evaluates dependencies,
defaults, reverse dependencies (`select` / `imply`), and `choice` exclusivity,
validates the result, and emits typed `CONFIG_*` constants. An illegal
configuration is a hard error.

## Use in a project

The crate is not on crates.io yet. Point Cargo at this repository.

### 1. Install the CLI (optional)

The binary is for `cargo kconfig check|build|test` from a terminal. Constants
in your crate do **not** need it.

```bash
cargo install --path /path/to/cargo-kconfig --force
cargo kconfig --help
```

### 2. Add Kconfig files

At the crate root (or the **workspace** root if you have several packages):

```text
my-firmware/
  Cargo.toml
  Kconfig
  qemu_defconfig          # or configs/qemu_defconfig
  src/lib.rs
```

```kconfig
# Kconfig
config FOO
	bool "Enable foo"
	default y

config BUFFER_SIZE
	int "Buffer size"
	range 1 4096
	default 128
	depends on FOO

config BOARD_NAME
	string "Board name"
	default "qemu"
```

```text
# qemu_defconfig
CONFIG_FOO=y
CONFIG_BUFFER_SIZE=256
CONFIG_BOARD_NAME="qemu-virt"
```

Put extra product files in `configs/` if you like. The plugin looks for
`defconfig` or exactly one `*_defconfig` in the project root and in
`configs/`. Several matches are an error unless you name one:

```bash
KCONFIG_DEFCONFIG=qemu_defconfig cargo build
# or
KCONFIG_DEFCONFIG=configs/prod_defconfig cargo build
```

Keep illegal assignment files out of that search (for example
`configs/cases/unmet_defconfig`) so a `cargo build` cannot pick them up.

### 3. Depend on the macros crate

No `build.rs`. Add a normal dependency so the proc-macro runs on the host
while your crate compiles (including `no_std` targets):

```toml
# Cargo.toml
[dependencies]
cargo-kconfig-macros = { path = "/path/to/cargo-kconfig/macros" }
```

From git, once you have a remote (Cargo finds the `macros` workspace member by
package name):

```toml
[dependencies]
cargo-kconfig-macros = { git = "https://github.com/<you>/cargo-kconfig" }
```

### 4. Include the generated constants

```rust
pub mod config {
    cargo_kconfig_macros::include_config!();
}

use config::*;

pub fn init() {
    if CONFIG_FOO {
        let _buf = [0u8; CONFIG_BUFFER_SIZE as usize];
        let _ = CONFIG_BOARD_NAME;
    }
}
```

`include_config!` finds `Kconfig` next to this package, or walks up to a
parent directory that has both `Cargo.toml` and `Kconfig`. It then evaluates
the unique defconfig (or `KCONFIG_DEFCONFIG`) and expands `CONFIG_*` items
into that module.

Use the **constants**, not rustc cfg:

```rust
if CONFIG_FOO { /* ... */ }          // yes
if !CONFIG_FOO { /* ... */ }

// #[cfg(CONFIG_FOO)]                // no — the macro cannot emit cargo:rustc-cfg
```

### 5. Workspace (several packages)

Cargo never runs a `build.rs` at a virtual workspace root. You do not need
one in each member either.

```text
my-product/
  Cargo.toml                 # [workspace] members = ["lib", "app"]
  Kconfig
  configs/qemu_defconfig
  lib/
    Cargo.toml               # cargo-kconfig-macros
    src/lib.rs               # pub mod config { include_config!(); }
  app/
    Cargo.toml               # depends on lib
    src/main.rs              # use my_lib::config::*;
```

`lib` owns the include. Other members depend on `lib` and write
`use my_lib::config::*;`. They do not need the macros crate or a `build.rs`.

```bash
cargo test -p my-lib
cargo run -p my-app
```

### 6. Check a tree from the CLI

From the project or a member directory:

```bash
cargo kconfig check
cargo kconfig build          # writes .config and config.rs next to the crate
```

`--root` / `--kconfig` / `--defconfig` override discovery. `check` does not
write files. An unmet `depends on`, bad `select`, `choice` conflict, or
out-of-range value prints a red error and exits 1.

### 7. Unit tests with other (including illegal) defconfigs

The compile-time include still uses the product defconfig. Tests that need a
**different** file, or a known-bad one, evaluate at runtime with `ConfigTest`.
Add the library as a **dev**-dependency so it is not linked into firmware:

```toml
[dependencies]
cargo-kconfig-macros = { path = "/path/to/cargo-kconfig/macros" }

[dev-dependencies]
cargo-kconfig = { path = "/path/to/cargo-kconfig" }
```

```rust
use cargo_kconfig::domain::{IssueKind, Value};
use cargo_kconfig::{ConfigTest, Error};

#[test]
fn uart_paths() {
    let kconfig = ConfigTest::discover().expect("workspace Kconfig");

    let ok = kconfig.evaluate("configs/qemu_defconfig").unwrap();
    if ok.get("UART") == Some(&Value::Bool(true)) {
        assert_eq!(ok.get("BUS"), Some(&Value::Bool(true)));
    }

    match kconfig.evaluate("configs/cases/unmet_defconfig") {
        Err(Error::Validation(report)) if report.has_kind(IssueKind::UnmetDependency) => {}
        other => panic!("expected unmet-dependency, got {other:?}"),
    }

    match kconfig.evaluate_text("CONFIG_UART=y\n") {
        Err(Error::Validation(_)) => {}
        other => panic!("expected validation error, got {other:?}"),
    }
}
```

### Optional: `#[cfg(CONFIG_*)]`

A proc-macro cannot emit `cargo:rustc-cfg`. If this crate must use
`#[cfg(CONFIG_FOO)]` / `cfg!(CONFIG_FOO)`, add `cargo-kconfig` as a
**build**-dependency and a four-line `build.rs`. That cfg is visible only
inside that crate.

```toml
[build-dependencies]
cargo-kconfig = { path = "/path/to/cargo-kconfig" }
```

```rust
fn main() {
    if cargo_kconfig::run_build_script().is_err() {
        std::process::exit(1);
    }
}
```

Prefer `if CONFIG_FOO` and sharing `the_lib::config` unless you truly need cfg.

## Commands

```bash
cargo kconfig check
cargo kconfig build
cargo kconfig test
```

`check` validates only. `build` writes `.config` and `config.rs`. `test`
additionally type-checks the generated constants with `rustc`.

```text
--root <DIR>           Project root used to resolve `source` paths
--kconfig <FILE>       Root Kconfig (default: <root>/Kconfig)
--defconfig <FILE>     Assignment file (`*_defconfig` or `.config`)
--out-dir <DIR>        Where to write generated files
--emit-rustc-cfg       Print cargo rustc-cfg lines (for an optional build.rs)
```

## Examples

| Example | What it shows |
| --- | --- |
| `examples/minimal` | Bool / int / string constants, `source`, and `if CONFIG_*` |
| `examples/depends` | `depends on`, `&&` / `||` / `!`, and `if` blocks. Switch with `KCONFIG_DEFCONFIG=no_bus_defconfig` |
| `examples/firmware` | Menus, `default ... if`, `range`, `tristate`, hex, `def_bool`, `select HAS_NET`, and a sourced file gated by `if NETWORK` |
| `examples/select` | `select` auto-enables helpers; enabling a target whose `depends on` is unmet is an error. `imply` is a weak default. Switch with `KCONFIG_DEFCONFIG=dma_defconfig` or `imply_defconfig` |
| `examples/choice` | Bool `choice` exclusivity. Switch with `KCONFIG_DEFCONFIG=rtt_defconfig` |
| `examples/errors` | Each keyword that must **fail**: `depends on`, `select`, `imply` (user `y`), `if`, `choice`, `range`. Valid defaults: `ok_defconfig` and `imply_respected_defconfig` |
| `examples/workspace` | Virtual workspace: root `Kconfig` + `configs/qemu_defconfig`, member `lib` with `include_config!` (no `build.rs`), member `app` that `use`s `lib::config`. Unit tests feed a bad defconfig through `ConfigTest` |

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

cargo test --manifest-path examples/workspace/lib/Cargo.toml
cargo run --manifest-path examples/workspace/app/Cargo.toml
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
