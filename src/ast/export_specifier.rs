/// One `local as exported` of an `export { ... }` list. `at` is the offset
/// plus one of `local` and `width` the characters from there to the end of the
/// specifier, which the `--node` link errors point at.
#[derive(Clone, Debug)]
pub(crate) struct ExportSpecifier {
    pub local: String,
    pub exported: String,
    pub at: u32,
    pub width: usize,
}
