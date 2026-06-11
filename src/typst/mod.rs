mod assets;
mod compiler;
mod diagnostics;
mod fonts;
mod memo;

pub(crate) use compiler::Compiler;
pub(crate) use fonts::FontCache;

use ::typst::syntax::{FileId, VirtualPath};

/// Builds a [`FileId`] for a virtual path within the in-memory file set.
pub(crate) fn file_id(path: &str) -> FileId {
    FileId::new(None, VirtualPath::new(path))
}
