fn main() {
    if cargo_kcfg::run_build_script().is_err() {
        std::process::exit(1);
    }
}
