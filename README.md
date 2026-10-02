# cargo-kcfg

Linux/Zephyr-style **Kconfig** for Rust crates.

Parse Kconfig definitions, load a `*_defconfig`, evaluate `depends on` /
`select` / `imply` / `choice` / `range`, and emit typed `CONFIG_*` constants.
An illegal configuration is a hard error: the tool does not silently repair
user intent.

This package is the library, the CLI, and the `build.rs` helper. A crate that
needs `CONFIG_*` constants depends on `cargo-kcfg` and calls `include_config!`.

## Installation

```bash
cargo install cargo-kcfg
cargo kcfg --help
```

The binary is named `cargo-kcfg`, so Cargo exposes it as a subcommand:

```bash
cargo kcfg check
cargo kcfg build
cargo kcfg test
```

Constants in your crate do **not** require the CLI. Use it to validate a tree
or to write `.config` / `config.rs` by hand.

## Use in a crate

### 1. Kconfig files

At the crate root (or the **workspace** root if you have several packages):

```text
my-firmware/
  Cargo.toml
  Kconfig
  qemu_defconfig          # or configs/qemu_defconfig
  src/lib.rs
```

```kconfig
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

The plugin looks for `defconfig` or exactly one `*_defconfig` in the project
root and in `configs/`. Several matches are an error unless you name one:

```bash
KCONFIG_DEFCONFIG=qemu_defconfig cargo build
KCONFIG_DEFCONFIG=configs/prod_defconfig cargo build
```

Keep illegal assignment files out of that search (for example
`configs/cases/unmet_defconfig`) so a `cargo build` cannot pick them up.

### 2. Depend on cargo-kcfg

`build.rs` runs on the host and writes `OUT_DIR/config.rs`. The normal
dependency is only the `include_config!` macro, so a `no_std` target does not
compile the evaluator:

```toml
[dependencies]
cargo-kcfg = { version = "0.0.1", default-features = false }

[build-dependencies]
cargo-kcfg = "0.0.1"
```

```rust,no_run
// build.rs
if cargo_kcfg::run_build_script().is_err() {
    std::process::exit(1);
}
```

### 3. Include the generated constants

```rust,ignore
pub mod config {
    cargo_kcfg::include_config!();
}

use config::*;

pub fn init() {
    if CONFIG_FOO {
        let _buf = [0u8; CONFIG_BUFFER_SIZE as usize];
        let _ = CONFIG_BOARD_NAME;
    }
}
```

`run_build_script` finds `Kconfig` next to this package, or walks up to a
parent directory that has both `Cargo.toml` and `Kconfig`. It then evaluates
the unique defconfig (or `KCONFIG_DEFCONFIG`) and writes `CONFIG_*` items.
`include_config!` pastes that file into the module.

Use the **constants**, not rustc cfg:

```rust,ignore
if CONFIG_FOO { /* ... */ }
if !CONFIG_FOO { /* ... */ }

// #[cfg(CONFIG_FOO)] also works: run_build_script emits cargo:rustc-cfg
```

### 4. Workspace (several packages)

Cargo never runs a `build.rs` at a virtual workspace root. Put it on the
member that owns the constants.

```text
my-product/
  Cargo.toml                 # [workspace] members = ["lib", "app"]
  Kconfig
  configs/qemu_defconfig
  lib/
    Cargo.toml               # cargo-kcfg = "0.0.1"
    build.rs                 # cargo_kcfg::run_build_script()
    src/lib.rs               # pub mod config { include_config!(); }
  app/
    Cargo.toml               # depends on lib
    src/main.rs              # use my_lib::config::*;
```

`lib` owns the include. Other members depend on `lib` and write
`use my_lib::config::*;`.

```bash
cargo test -p my-lib
cargo run -p my-app
```

### 5. Check a tree from the CLI

```bash
cargo kcfg check
cargo kcfg build          # writes .config and config.rs next to the crate
```

`--root` / `--kconfig` / `--defconfig` override discovery. `check` does not
write files. An unmet `depends on`, bad `select`, `choice` conflict, or
out-of-range value prints a red error and exits 1.

### 6. Unit tests with other (including illegal) defconfigs

The compile-time include still uses the product defconfig. Tests that need a
**different** file, or a known-bad one, evaluate at runtime with `ConfigTest`:

```toml
[dependencies]
cargo-kcfg = { version = "0.0.1", default-features = false }

[build-dependencies]
cargo-kcfg = "0.0.1"

[dev-dependencies]
cargo-kcfg = "0.0.1"
```

```rust,no_run
use cargo_kcfg::domain::{IssueKind, Value};
use cargo_kcfg::{ConfigTest, Error};

fn uart_paths() -> Result<(), Error> {
    let kconfig = ConfigTest::discover()?;

    let ok = kconfig.evaluate("configs/qemu_defconfig")?;
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
    Ok(())
}
```

### Optional: `#[cfg(CONFIG_*)]`

The same `build.rs` prints `cargo:rustc-cfg` for enabled bool and tristate
symbols. `#[cfg(CONFIG_FOO)]` / `cfg!(CONFIG_FOO)` are visible only inside the
crate that runs the build script. Prefer `if CONFIG_FOO` and sharing
`the_lib::config` unless you truly need cfg.

## Crate map

| Surface | Responsibility |
| --- | --- |
| `include_config!` | Pastes `OUT_DIR/config.rs` into the calling module. |
| `ConfigTest` | Runtime evaluate of a chosen (including illegal) defconfig from unit tests. |
| `Pipeline` / `ProjectLocator` | Load Kconfig + defconfig, evaluate, generate `.config` / `config.rs`. |
| `BuildScript` / `run_build_script` | `build.rs` helper: write `OUT_DIR` artefacts and emit `cargo:rustc-cfg`. |
| `cli::Cli` | `cargo kcfg check\|build\|test`. |
| `Telemetry` | Process-wide tracing on stderr; `new(prefix)` prefixes every log line. |
| `domain` | Pure evaluation: symbols, expressions, `select`/`imply`/`choice`. No filesystem. |
| `infra` | Files, parser adapter, codegen, discovery. Depends on `domain`, never the reverse. |
| `Error` | All library and CLI failures. Domain evaluation failures are `ValidationReport`. |

The binary (`src/main.rs`) only installs `Telemetry` and calls `cli::Cli::run`. It panics solely if telemetry cannot be installed.

## Errors

Public functions return `Result` with `Error` (or `TelemetryError` / `ValidationReport` where noted). Nothing in the library panics on user input.

| Variant | When |
| --- | --- |
| `Error::Io` | A Kconfig, defconfig, or output path could not be read or written. |
| `Error::Parse` | `nom-kconfig` rejected the file, or trailing text was left unparsed. |
| `Error::Validation` | The configuration is incoherent (unmet `depends on`, bad `select`, `choice` conflict, unknown symbol, type mismatch, out of range, cycle). |
| `Error::Domain` | Resource limits or symbol-table rules (`Limits`) were exceeded. |
| `Error::Usage` | Missing files, ambiguous defconfigs, missing `CARGO_MANIFEST_DIR`/`OUT_DIR`, or `rustc` failed during `cargo kcfg test`. |
| `TelemetryError::InitFailed` | `Telemetry::new` called more than once in the process. |

Match on `Error::Validation` and `ValidationReport::has_kind` in tests to branch on a known-bad defconfig. The `Display` text always includes what is wrong and what to change.

## Commands

```bash
cargo kcfg check
cargo kcfg build
cargo kcfg test
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

## Examples in this repository

| Example | What it shows |
| --- | --- |
| `examples/minimal` | Bool / int / string constants, `source`, and `if CONFIG_*` |
| `examples/depends` | `depends on`, `&&` / `||` / `!`, and `if` blocks. Switch with `KCONFIG_DEFCONFIG=no_bus_defconfig` |
| `examples/firmware` | Menus, `default ... if`, `range`, `tristate`, hex, `def_bool`, `select HAS_NET`, and a sourced file gated by `if NETWORK` |
| `examples/select` | `select` auto-enables helpers; enabling a target whose `depends on` is unmet is an error. `imply` is a weak default. Switch with `KCONFIG_DEFCONFIG=dma_defconfig` or `imply_defconfig` |
| `examples/choice` | Bool `choice` exclusivity. Switch with `KCONFIG_DEFCONFIG=rtt_defconfig` |
| `examples/errors` | Each keyword that must **fail**: `depends on`, `select`, `imply` (user `y`), `if`, `choice`, `range`. Valid defaults: `ok_defconfig` and `imply_respected_defconfig` |
| `examples/workspace` | Virtual workspace: root `Kconfig` + `configs/qemu_defconfig`, member `lib` with `build.rs` and `include_config!`, member `app` that `use`s `lib::config`. Unit tests feed a bad defconfig through `ConfigTest` |

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
cargo kcfg check --root examples/errors --defconfig depends_unmet_defconfig
cargo kcfg check --root examples/errors --defconfig select_unmet_defconfig
cargo kcfg check --root examples/errors --defconfig imply_unmet_defconfig
cargo kcfg check --root examples/errors --defconfig if_unmet_defconfig
cargo kcfg check --root examples/errors --defconfig choice_conflict_defconfig
cargo kcfg check --root examples/errors --defconfig range_bad_defconfig
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

## License

MIT
