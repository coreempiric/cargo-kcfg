#![doc = include_str!("../README.md")]

use cargo_kconfig::{Pipeline, ProjectLocator};
use proc_macro::TokenStream;
use quote::quote;
use std::env;
use std::path::{Path, PathBuf};

/// Expand the evaluated `config.rs` into the calling module.
///
/// Takes no arguments. Discovers `Kconfig` from `CARGO_MANIFEST_DIR` (walking
/// up to a workspace root) and the unique `*_defconfig` or `KCONFIG_DEFCONFIG`.
///
/// # Errors
///
/// Expansion becomes `compile_error!` when:
///
/// - the macro is invoked with arguments
/// - `CARGO_MANIFEST_DIR` is unset
/// - `Kconfig` or a unique defconfig cannot be found
/// - the definition file cannot be parsed
/// - evaluation is incoherent (unmet `depends on`, bad `select`, and so on)
/// - generated Rust is not valid tokens
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
