fn main() {
    if cargo_kconfig::run_build_script().is_err() {
        std::process::exit(1);
    }
}
