//! Formatting of Typst compilation diagnostics into human-readable messages.

use ::typst::diag::{Severity, SourceDiagnostic};
use ::typst::syntax::DiagSpan;
use ::typst::{World, WorldExt};

use super::compiler::Compiler;

/// Joins compilation diagnostics into a single human-readable message.
///
/// Each diagnostic carries its severity, source location, and any hints Typst
/// provides for resolving it.
pub fn format_diagnostics(world: &Compiler, diagnostics: &[SourceDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format_diagnostic(world, diagnostic))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats a single diagnostic as `path:line:column: severity: message`,
/// followed by an indented line per hint.
pub fn format_diagnostic(world: &Compiler, diagnostic: &SourceDiagnostic) -> String {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };

    let mut formatted = match location(world, diagnostic.span) {
        Some(location) => format!("{location}: {severity}: {}", diagnostic.message),
        None => format!("{severity}: {}", diagnostic.message),
    };

    for hint in &diagnostic.hints {
        formatted.push_str(&format!("\n  hint: {}", hint.v));
    }

    formatted
}

/// Resolves a span to a `path:line:column` location relative to the compilation
/// root, or `None` if the span does not point into a file.
fn location(world: &Compiler, span: DiagSpan) -> Option<String> {
    let id = span.id()?;
    let source = world.source(id).ok()?;
    let range = world.range(span)?;
    let (line, column) = source.lines().byte_to_line_column(range.start)?;
    let path = id.vpath().get_without_slash();
    Some(format!("{path}:{}:{}", line + 1, column + 1))
}
