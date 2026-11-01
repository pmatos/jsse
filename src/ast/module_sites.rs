//! IC site numbering of a module's top-level items.

use super::*;

/// Assign dense IC site ids to all call/new/member sites in a module-level
/// program. The module's top-level items are not stored in a `Body`, but they
/// share a single dense namespace keyed by the program's `body` field. This
/// keeps IC sites on module top-level executable expressions valid while the
/// interpreter is executing module items.
pub(crate) fn assign_ic_sites_for_module(program: &mut Program) {
    if program.source_type == SourceType::Script {
        assign_ic_sites(&mut program.body);
        return;
    }

    let mut call_id = 0u32;
    let mut prop_id = 0u32;
    for item in program.module_items.iter_mut() {
        assign_module_item_sites(item, &mut call_id, &mut prop_id);
    }
    program.body.ic.call_site_count = call_id;
    program.body.ic.prop_site_count = prop_id;
    program.body.ic.assigned = true;
}

fn assign_module_item_sites(item: &mut ModuleItem, call_id: &mut u32, prop_id: &mut u32) {
    match item {
        ModuleItem::Statement(stmt) => assign_stmt_sites(stmt, call_id, prop_id),
        ModuleItem::ImportDeclaration(_) => {}
        ModuleItem::ExportDeclaration(export) => assign_export_sites(export, call_id, prop_id),
    }
}

fn assign_export_sites(export: &mut ExportDeclaration, call_id: &mut u32, prop_id: &mut u32) {
    match export {
        ExportDeclaration::Named { declaration, .. } => {
            if let Some(decl) = declaration.as_mut() {
                assign_stmt_sites(decl, call_id, prop_id);
            }
        }
        ExportDeclaration::Default(expr) => assign_expr_sites(expr, call_id, prop_id),
        ExportDeclaration::DefaultFunction(f) => {
            assign_ic_sites(&mut f.body);
        }
        ExportDeclaration::DefaultClass(c) => assign_class_sites(c, call_id, prop_id),
        ExportDeclaration::All { .. } => {}
    }
}
