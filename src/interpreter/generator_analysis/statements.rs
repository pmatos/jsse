use super::*;

pub(super) fn analyze_statements(
    stmts: &[Statement],
    analysis: &mut GeneratorAnalysis,
    ctx: &mut AnalysisContext,
) {
    for stmt in stmts {
        analyze_statement(stmt, analysis, ctx);
    }
}
