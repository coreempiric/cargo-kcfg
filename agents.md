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
