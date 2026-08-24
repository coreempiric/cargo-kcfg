//! Compile-time Kconfig include. Replaces a consumer `build.rs`.
//!
//! ```ignore
//! pub mod config {
//!     cargo_kconfig_macros::include_config!();
//! }
//! ```
//!
//! A virtual workspace root cannot host a `build.rs` (Cargo never runs it).
//! This macro runs while compiling the member crate, locates `Kconfig` the
//! same way [`cargo_kconfig::BuildScript`] does, and expands the `CONFIG_*`
//! constants in place. It cannot emit `cargo:rustc-cfg`; use `if CONFIG_FOO`
//! (or keep an optional `build.rs` that calls `run_build_script` if you need
//! `#[cfg(CONFIG_FOO)]`).

use cargo_kconfig::{Pipeline, ProjectLocator};
use proc_macro::TokenStream;
use quote::quote;
use std::env;
use std::path::{Path, PathBuf};

/// Expand the evaluated `config.rs` into the calling module.
#[proc_macro]
pub fn include_config(input: TokenStream) -> TokenStream {
    if !input.is_empty() {
        return compile_error("include_config! takes no arguments");
    }
    match expand() {
        Ok(tokens) => tokens,
        Err(message) => compile_error(&message),
    }
}

fn compile_error(message: &str) -> TokenStream {
    let lit = proc_macro2::Literal::string(message);
    quote! { compile_error!(#lit); }.into()
}

fn expand() -> Result<TokenStream, String> {
    let manifest = env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        "CARGO_MANIFEST_DIR is not set. include_config! is for Cargo crates".to_string()
    })?;
    let locator =
        ProjectLocator::discover(PathBuf::from(manifest)).map_err(|err| err.to_string())?;
    let request = locator.resolve(None, None).map_err(|err| err.to_string())?;
    let result = Pipeline::new()
        .run(&request)
        .map_err(|err| err.to_string())?;

    let body: proc_macro2::TokenStream = result
        .generated
        .config_rs
        .parse()
        .map_err(|err| format!("generated config.rs is not valid Rust: {err}"))?;

    let mut files = Vec::new();
    push_tracked(&mut files, &request.kconfig);
    push_tracked(&mut files, &request.defconfig);
    for loaded in &result.loaded_files {
        push_tracked(&mut files, loaded);
    }
    files.sort();
    files.dedup();
    let file_lits = files
        .iter()
        .map(|p| proc_macro2::Literal::string(p))
        .collect::<Vec<_>>();

    Ok(quote! {
        #body
        const _: &[&str] = &[#(include_str!(#file_lits)),*];
        const _: Option<&str> = option_env!("KCONFIG_DEFCONFIG");
    }
    .into())
}

fn push_tracked(files: &mut Vec<String>, path: &Path) {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(text) = abs.to_str() {
        files.push(text.to_string());
    }
}
