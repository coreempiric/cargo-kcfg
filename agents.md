# agents.md

## Project Vision

Build a **Cargo extension** (`cargo-kcfg` or equivalent) that brings robust Linux/Zephyr-style Kconfig support to Rust projects.

The tool must produce a configuration that is **known to be coherent**.  
If the configuration is inconsistent, the tool **errors out** with a clear message.  
A successful run is a guarantee that the selected set of options is valid and safe to build against.

### Core pipeline

1. Parse Kconfig **definition** files (prefer `nom-kconfig`).
2. Load user assignments from `*_defconfig` (or equivalent assignment files).
3. Perform full **value evaluation** and **dependency resolution**, including automatic enabling of symbols via reverse dependencies.
4. **Validate** the resulting configuration strictly.
5. On success, emit:
   - A final `.config` (debugging / compatibility)
   - A generated `config.rs` with typed `const` items
   - `cargo:rustc-cfg=...` flags for boolean/tristate options

On inconsistency the tool must **fail with an error**, not merely warn.
`select` still automatically enables its target, but if that target's
`depends on` is unmet the run fails with a message telling the user to
enable the missing dependencies or remove the `select`.

Primary commands (illustrative):

```bash
cargo kcfg check
cargo kcfg build
cargo kcfg test
```

Generated constants must be directly usable:

```rust
if CONFIG_FOO {
    let buf = [0u8; CONFIG_BUFFER_SIZE as usize];
    println!("Board: {}", CONFIG_BOARD_NAME);
}
```

---

## Language & Type Rules

- Implementation language: **Rust**.
- Prefer **explicit, platform-independent types**:
  - `u8`, `u16`, `u32`, `u64`, `usize` where appropriate
  - Prefer `u32` for configuration integers
  - `&'static str` for string options
  - A clear `Tristate` enum (`n` / `m` / `y`) for boolean/tristate logic
- Avoid implicit widening/narrowing.
- Avoid platform-dependent C-style types (`int`, `long`, etc.).
- All public APIs and generated constants must use explicit types.

---

## Engineering Practices

### Test-Driven Development (TDD)

- Write failing tests first for new behaviour.
- Keep the feedback loop short.
- Prefer small, focused unit tests for pure domain logic (expression evaluation, dependency resolution, validation rules).
- Every new evaluation rule, reverse-dependency behaviour, or validation check must land with tests.

### Domain-Driven Design (DDD)

Model the core domain explicitly:

- `Symbol`
- `SymbolTable`
- `Expression`
- `Tristate` / `Value`
- `Defconfig` / assignment set
- `EvaluationContext`
- `DependencyGraph` (direct + reverse)
- `ValidationReport` / structured errors

Keep the domain layer free of Cargo-specific or filesystem details.  
Infrastructure (file loading, Cargo integration, code generation) depends on the domain, not the other way around.

### Defensive Programming

- Validate all inputs at system boundaries (Kconfig files, `*_defconfig` files, CLI arguments).
- Prefer returning `Result` with descriptive errors over panics in library code.
- Treat unexpected symbol types, out-of-range values, unmet dependencies
  (including after `select`), and illegal reverse-dependency targets
  (unknown or non-bool/tristate) as **hard errors**.
- Never assume a `.config` / `defconfig` value is well-formed; always check.
- Use exhaustive matching on enums so new variants become compile-time errors.

### Buffer & Resource Limitations

- Do not assume unbounded input sizes.
- Apply reasonable limits to:
  - Maximum length of symbol names
  - Maximum length of string config values
  - Maximum number of symbols
  - Maximum recursion depth when following `source` directives
  - Maximum expression depth/complexity
  - Maximum dependency-resolution iterations (detect cycles / non-convergence)
- Fail with clear errors when limits are exceeded rather than growing without bound.

---

## Dependency & Automatic Enabling Semantics

The evaluator must correctly handle mechanisms that automatically turn other configs on (or constrain them), including:

| Mechanism     | Role |
|---------------|------|
| `depends on`  | Direct dependency / upper bound / visibility |
| `select`      | Reverse dependency that forces a lower bound on another symbol |
| `imply`       | Weaker reverse dependency (soft lower bound) |
| `default`     | Conditional default values |
| `range`       | Numeric constraints on `int` / `hex` |
| `choice`      | Mutually related groups of symbols (exactly one bool member, or tristate `m`/`y` rules) |
| `visible if`  | Visibility control for menus / entries |

### Strictness policy

- Do **not** silently drop or invent values.
- **Error out** with a precise message when:
  - `select` / `imply` names an unknown symbol or a non-bool/tristate target
  - A symbol is enabled while `depends on` is unmet (whether from a user
    assignment or from `select`)
  - A value is out of range
  - A type is incorrect
  - A symbol is unknown
  - Resolution does not converge (cycle or conflict)
  - A choice constraint is violated (two members `y`, or none selected when the choice is not `optional`)

A successful run means the configuration is coherent and usable for a build.

---

## Core Behaviour to Support

### Parsing
- Use a maintained Kconfig definition parser (`nom-kconfig` preferred).
- Follow `source` directives recursively from a root Kconfig.  
  Do **not** blindly crawl the filesystem for every `Kconfig*` file.
- Preserve expression ASTs so the evaluator can walk them.

### Evaluation
- Build a complete symbol table from the parsed definitions.
- Load assignments from `*_defconfig`.
- Evaluate expressions to tristate or concrete values.
- Resolve direct dependencies (`depends on`).
- Process reverse dependencies (`select`, `imply`) so that forced symbols are enabled or constrained automatically.
- Apply conditional defaults.
- Iterate to a fixed point; detect non-convergence and error.
- Enforce ranges for `int` / `hex`.

### Validation (strict)
- Unknown symbols → error
- Type mismatches → error
- Out-of-range values → error
- Unmet dependencies after resolution (user assignment or `select`) → error
- `select` / `imply` of unknown or non-bool/tristate targets → error
- Conflicting constraints → error
- Clear, actionable messages that name the symbols involved

### Code Generation
- Emit typed `const` items in `config.rs` (prefer `const` over `static`).
- Emit a human-readable final `.config`.
- Emit `rustc-cfg` flags for enabled boolean/tristate options.

---

## Examples

The repository must contain working examples that demonstrate:

- A minimal Kconfig + `*_defconfig` project
- Reading generated `CONFIG_*` constants from Rust code
- Using integer config values for buffer sizes / array lengths
- Using string config values
- Conditional compilation via generated `cfg` flags
- Cases where `select` / `imply` correctly force other symbols on
- Cases where `select` would enable a target with unmet `depends on` and the tool errors
- Cases where illegal combinations (`depends on`, `select`, `imply`, `if`, `choice`, `range`) are rejected with errors

Examples should be buildable and serve as living documentation.

---

## Testing Strategy

### Unit Tests
- Expression evaluation (`&&`, `||`, `!`, comparisons, symbol references)
- Direct dependency resolution
- Reverse dependency handling (`select`, `imply`)
- Default application
- Type checking and range validation
- Fixed-point / convergence behaviour
- Error cases (conflicts, unmet deps, bad types, unknown symbols)

### Integration Tests

The `tests/` (or `integration_tests/`) folder must contain multiple end-to-end tests that verify real behaviour, including:

- Loading a root Kconfig that `source`s additional files
- Parsing a `*_defconfig` and producing a correct final `.config`
- Automatic enabling of symbols via `select` / `imply`
- Rejecting `select` of a target whose `depends on` is unmet
- Rejecting configurations that leave dependencies unmet
- Rejecting `select` / `imply` of unknown or non-bool/tristate targets
- Rejecting unknown symbols, type-incorrect values, and out-of-range values
- Generating `config.rs` with the expected typed constants
- Verifying that generated constants can be used from Rust code (array sizes, conditionals, etc.)
- Checking that `source` following is driven by the Kconfig graph, not by directory crawling
- Detecting and erroring on conflicting or non-convergent configurations

Integration tests should use fixture directories that contain realistic but small Kconfig trees and defconfig files, including both success and deliberate failure cases.

---

## Non-Goals (for the first complete product drop)

- Interactive `menuconfig`-style TUI (may come later)
- 100% behavioural identity with every historical Linux Kconfig quirk on day one
- Blind filesystem crawling as the primary loading strategy
- Silent repair of invalid configurations (the tool must not quietly “fix”
  user intent; `select` that would ignore `depends on` must error)

The product may grow toward broader compatibility, but correctness and strict failure on inconsistency remain non-negotiable.

---

## Success Criteria

- A user can define options in Kconfig files (including `depends on`, `select`, `imply`, defaults, ranges).
- A user can assign values in a `*_defconfig` file.
- The tool evaluates the full configuration, applying automatic enabling where required.
- Inconsistency causes a **hard error**, including `select` of a target
  whose `depends on` is unmet.
- On success the tool produces `.config` and `config.rs`.
- Generated constants are usable in normal Rust code with explicit types (`u32`, `bool`, `&'static str`, etc.).
- A successful run is a reliable signal that the configuration is coherent and suitable for a proper build.
- The test suite (unit + integration) exercises both success paths and the major failure modes.
- The design follows TDD, keeps a clear domain model (DDD), and applies defensive checks and resource limits.



### Description

`run_build_script` currently assumes a single package. It expects `Kconfig`
and a unique `*_defconfig` next to that package’s `Cargo.toml`.

Many Rust products instead use a virtual Cargo workspace:

- The root `Cargo.toml` contains only `[workspace]` (no `[package]`).
- `Kconfig` and `configs/*_defconfig` live at the repository root.
- Member crates (`lib`, `app`, `tests`, …) live in subdirectories.

Cargo does not run a `build.rs` at a virtual workspace root. `OUT_DIR` is
per-crate. An `include!(concat!(env!("OUT_DIR"), "/config.rs"))` in a
member therefore reads only that member’s `OUT_DIR`, which only that
member’s own `build.rs` can populate. `cargo:rustc-cfg` emitted by one
crate’s build script is invisible when compiling a dependent. `#[path]`
accepts only a string literal, so it cannot reference `OUT_DIR`.

Without first-class workspace support the plugin forces every member that
needs the generated constants to:

1. Hand-roll `ProjectLocator` / `Pipeline` paths up to the workspace
   `Kconfig`.
2. Keep a `build.rs` next to its own `Cargo.toml`.
3. Host an `include!` of its private `OUT_DIR`.

That is more overhead than the five-line `build.rs` shown in the README.

### Responsibilities

Today the plugin is responsible for:

- Resolving `Kconfig` as `<CARGO_MANIFEST_DIR>/Kconfig`.
- Resolving one assignment file from (in order) `KCONFIG_DEFCONFIG`, a
  unique `*_defconfig` in the manifest directory or `configs/`, or an
  explicit `--defconfig` path.
- Writing `config.rs` and `.config` into `OUT_DIR` from
  `run_build_script`.
- Emitting `cargo:rustc-cfg` and `cargo:rustc-check-cfg` for enabled
  boolean and tristate symbols (visible only inside that crate).
- Exposing the include path
  `include!(concat!(env!("OUT_DIR"), "/config.rs"))` as the supported way
  to bring `CONFIG_*` constants into a module
  (`pub mod config { include!(...); }`).

The CLI subcommands (`check`, `build`, `test`) already accept `--root`
independently of Cargo workspaces. The build-script helper does not.

### Lacking

Workspace discovery, a zero-`build.rs` include path, and cross-crate cfg
visibility are not implemented. Candidate approaches:

| Approach | Pros | Cons |
| --- | --- | --- |
| Workspace locator inside `run_build_script` | Preserves the five-line `build.rs`. Walk upward from `CARGO_MANIFEST_DIR` (or use `CARGO_WORKSPACE_DIR` when available) until a workspace `Cargo.toml` + `Kconfig` + `configs/*_defconfig` are found. Members continue to `include!` their own `OUT_DIR`. | Users still need `cargo-kcfg` as a build-dependency and a `build.rs`. Each member that includes constants still generates into its private `OUT_DIR`. |
| Proc-macro `kconfig::include!()` | No consumer `build.rs`. The macro locates the workspace `Kconfig`, evaluates, and expands the `CONFIG_*` constants. Crate root becomes `pub mod config { kconfig::include!(); }`; dependents simply `use lib::config::*;`. | A proc-macro cannot emit `cargo:rustc-cfg`. `#[cfg(CONFIG_FOO)]` therefore requires a build script, or code must use the `bool` constant (`if CONFIG_FOO`). |
| `cargo kcfg init` | Writes the five-line `build.rs` and the `mod config` include so the user never copies them from the README. | Still produces a `build.rs` per crate that needs generation. Does not by itself locate a workspace `Kconfig`. |
| Dedicated config crate in the workspace | One member owns the `build.rs` and `pub mod config { include!(...); }`. Other members depend on it and `use config_crate::*;` (or `use lib::config::*;`). Values are shared. | `#[cfg(CONFIG_FOO)]` still does not propagate across the dependency edge. The extra crate is workspace boilerplate the plugin can document but should not require. |
| Root package instead of a virtual workspace | Root `Cargo.toml` has `[package]` and `[lib] path = "lib/src/lib.rs"`. Root `build.rs` runs, `CARGO_MANIFEST_DIR` is the repository root, and `run_build_script` finds `Kconfig` with no walk. | This is a Cargo layout change for the product, not plugin behaviour. It does not help users who prefer a virtual workspace. |
| Generate a source-relative `config.rs` | `#[path = "configs/config.rs"] pub mod config;` works because `#[path]` requires a string literal. rustfmt can open the file. | Generated Rust in the source tree is easy to commit by mistake and fights the `OUT_DIR` model. Prefer `include!` of `OUT_DIR` or a proc-macro until the plugin owns a stable path. |

**Preferred order of work**

1. Implement the workspace locator so `run_build_script` is usable from a
   member crate.
2. Consider a proc-macro so `build.rs` becomes optional for constant
   access.
3. Document that `#[cfg(CONFIG_*)]` is per-crate unless that crate’s own
   build script emits the cfg lines.
4. Add a workspace integration-test example (root `Kconfig`,
   `configs/*_defconfig`, member `lib` that includes the generated
   constants).

**Non-goals / explicit constraints**

- Do not treat a `build.rs` at a virtual workspace root as supported;
  Cargo ignores it.
- Do not expect one crate’s `rustc-cfg` to apply to another crate.
- Do not promise `#[path = concat!(env!("OUT_DIR"), "/config.rs")]`;
  rustc requires a string-literal path.
