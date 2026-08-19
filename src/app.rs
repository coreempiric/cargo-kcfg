//! Application facade — start here after `main`.
//!
//! Call chain:
//!
//! ```text
//! main
//!   └─ Application::run
//!        └─ Cli::run                         (src/cli.rs)
//!             └─ Commands::execute
//!                  ├─ CheckCommand::execute
//!                  ├─ BuildCommand::execute
//!                  └─ TestCommand::execute   (build + rustc type-check)
//!                       ├─ ProjectLocator::resolve     (src/infra/locator.rs)
//!                       ├─ Pipeline::run               (src/infra/pipeline.rs)
//!                       │    ├─ KconfigLoader::load    (src/infra/kconfig.rs)
//!                       │    ├─ DefconfigLoader::load  (src/infra/defconfig.rs)
//!                       │    ├─ Evaluator::evaluate    (src/domain/evaluation.rs)
//!                       │    └─ CodeGenerator::generate (src/infra/codegen.rs)
//!                       ├─ ArtifactWriter::write
//!                       └─ GeneratedSourceChecker::typecheck   (test only)
//!
//! crate build.rs
//!   └─ BuildScript::run                    (src/infra/build_script.rs)
//!        └─ same locator → pipeline → writer path
//! ```

use crate::cli::Cli;
use crate::error::Error;

/// Facade over the CLI. `main` calls [`Application::run`] and nothing else.
pub struct Application;

impl Application {
    /// Parse argv and run `check`, `build`, or `test`.
    pub fn run() -> Result<(), Error> {
        Cli::run()
    }
}
