//! Module link errors as V8 and Node report them (issue #1484). An import of a
//! name the target module does not export is a `SyntaxError` that reads
//! `The requested module './lib.mjs' does not provide an export named 'x'`
//! (the specifier as written), and Node prefixes the stack with the source
//! line and carets under the name (`decorateErrorStack`). The prefix is kept in
//! a hidden slot of the error and added by the prelude when `stack` is first
//! read. Default mode keeps its own wording.

use super::*;
use crate::interpreter::ModuleKey;

const ARROW: PrivateId = PrivateId(u64::MAX - 6);

/// The import being linked: the specifier as written, the offset plus one and
/// the width of the name that asks for the export, and the importing module.
#[derive(Clone)]
pub(crate) struct ImportName {
    specifier: String,
    at: u32,
    width: usize,
    importer: Option<ModuleKey>,
}

impl Interpreter {
    /// Runs `link` for one imported name, so a missing export can say which
    /// specifier asked for it and where.
    pub(crate) fn with_import_name<T>(
        &mut self,
        specifier: &str,
        (at, width): (u32, usize),
        importer: Option<&ModuleKey>,
        link: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if !self.node_host.stack.installed {
            return link(self);
        }
        let name = ImportName {
            specifier: specifier.to_string(),
            at,
            width,
            importer: importer.cloned(),
        };
        let saved = self.node_host.stack.import_name.replace(name);
        let result = link(self);
        self.node_host.stack.import_name = saved;
        result
    }

    /// The `SyntaxError` for `name` missing from `module`.
    pub(crate) fn no_export_error(&mut self, module: &ModuleKey, name: &str) -> JsValue {
        let Some(import) = self.node_host.stack.import_name.clone() else {
            let message = format!("Module '{}' has no export named '{name}'", module.display());
            return self.create_error("SyntaxError", &message);
        };
        // Node links in a later job than the one that loads, so the error is
        // not made inside the `import()` call.
        let site = self.node_host.stack.import_site.take();
        let message = format!(
            "The requested module '{}' does not provide an export named '{name}'",
            import.specifier
        );
        let error = self.create_error("SyntaxError", &message);
        self.node_host.stack.import_site = site;
        if let (Some(importer), Some(id)) = (&import.importer, error.as_object_id()) {
            self.decorate_with_source_line(id, importer, import.at, import.width);
        }
        error
    }

    /// Notes on `error` the line of `importer` at offset `at - 1`, with `width`
    /// carets under it, as Node's arrow message.
    fn decorate_with_source_line(
        &mut self,
        error: u64,
        importer: &ModuleKey,
        at: u32,
        width: usize,
    ) {
        let Some(origin) = self.node_host.stack.module_origins.get(importer).cloned() else {
            return;
        };
        let id = self.node_host.stack.source_id(&origin);
        let Some((Some(file), line, _)) = self.node_host.stack.locate(id, at) else {
            return;
        };
        let offset = (at as usize).saturating_sub(1).min(origin.len());
        let start = origin[..offset].rfind(['\n', '\r']).map_or(0, |n| n + 1);
        let end = origin[offset..]
            .find(['\n', '\r'])
            .map_or(origin.len(), |n| offset + n);
        let padding: String = origin[start..offset]
            .chars()
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        let arrow = format!(
            "{file}:{line}\n{}\n{padding}{}\n",
            &origin[start..end],
            "^".repeat(width)
        );
        if let Some(handle) = self.get_object_cell(error) {
            handle
                .borrow_mut()
                .set_hidden_slot(ARROW, string_value(&arrow));
        }
    }

    pub(super) fn setup_arrow_hook(&mut self) {
        let arrow = self.create_function(JsFunction::native(
            "__host_stack_arrow".to_string(),
            1,
            |interp, _this, args| {
                let slot = args
                    .first()
                    .and_then(JsValue::as_object_id)
                    .and_then(|id| interp.get_object(id))
                    .and_then(|obj| obj.borrow().hidden_slot(ARROW));
                Completion::Normal(slot.unwrap_or(JsValue::UNDEFINED))
            },
        ));
        self.install_host_global("__host_stack_arrow", arrow);
    }
}
