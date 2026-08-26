/// Resource limits applied at system boundaries and during evaluation.
///
/// Exceeding a limit is a [`crate::domain::DomainError`], never unbounded growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_symbol_name_len: usize,
    pub max_string_value_len: usize,
    pub max_symbols: usize,
    pub max_source_depth: usize,
    pub max_expression_depth: usize,
    pub max_file_bytes: u64,
    /// Cap on select/imply/choice fixed-point iterations.
    pub max_resolution_iterations: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_symbol_name_len: 256,
            max_string_value_len: 4_096,
            max_symbols: 50_000,
            max_source_depth: 64,
            max_expression_depth: 256,
            max_file_bytes: 10 * 1024 * 1024,
            max_resolution_iterations: 4096,
        }
    }
}
