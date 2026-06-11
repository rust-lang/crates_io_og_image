//! The [`World`] environment the Typst compiler reads from and the rendering of
//! the resulting document to a PNG.

use std::collections::HashMap;
use std::sync::Arc;

use ::typst::diag::{FileError, FileResult, Warned};
use ::typst::foundations::{Bytes, Datetime, Dict};
use ::typst::layout::PagedDocument;
use ::typst::syntax::{FileId, Source};
use ::typst::text::{Font, FontBook};
use ::typst::utils::LazyHash;
use ::typst::{Library, LibraryExt, World};

use super::assets::{ASSETS, MAIN_SOURCE};
use super::diagnostics::{format_diagnostic, format_diagnostics};
use super::fonts::FontCache;
use super::memo::EvictGuard;
use crate::error::OgImageError;

/// Resolution used for PNG export.
const PIXELS_PER_POINT: f32 = 2.;

/// The environment the Typst compiler reads from.
///
/// The main template comes from the shared [`MAIN_SOURCE`] and the bundled
/// assets from [`ASSETS`]; only the per-request downloaded avatars are
/// held here, keyed by the virtual paths the template references.
pub struct Compiler {
    library: LazyHash<Library>,
    avatars: HashMap<FileId, Bytes>,
    fonts: Arc<FontCache>,
}

impl Compiler {
    /// Builds the compilation environment.
    ///
    /// `inputs` is the `sys.inputs` dictionary; `avatars` are the downloaded
    /// avatar files keyed by the virtual path the template references.
    pub fn new(fonts: Arc<FontCache>, inputs: Dict, avatars: HashMap<FileId, Bytes>) -> Self {
        Self {
            library: LazyHash::new(Library::builder().with_inputs(inputs).build()),
            avatars,
            fonts,
        }
    }

    /// Compiles the template to a PNG image.
    pub fn compile_png(&self) -> Result<Vec<u8>, OgImageError> {
        let _evict = EvictGuard { max_age: 5 };

        let Warned { output, warnings } = ::typst::compile::<PagedDocument>(self);

        for warning in &warnings {
            tracing::warn!(diagnostic = %format_diagnostic(self, warning), "Typst compilation warning");
        }

        let document = output.map_err(|diagnostics| {
            OgImageError::TypstCompilation(format_diagnostics(self, &diagnostics))
        })?;

        let page = document
            .pages
            .first()
            .ok_or_else(|| OgImageError::TypstCompilation("Typst produced no pages".into()))?;

        let pixmap = typst_render::render(page, PIXELS_PER_POINT);
        let png = pixmap.encode_png().map_err(|err| {
            OgImageError::TypstCompilation(format!("failed to encode PNG: {err}"))
        })?;

        Ok(png)
    }
}

impl World for Compiler {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.fonts.book
    }

    fn main(&self) -> FileId {
        MAIN_SOURCE.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == MAIN_SOURCE.id() {
            Ok(MAIN_SOURCE.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        ASSETS
            .get(&id)
            .or_else(|| self.avatars.get(&id))
            .cloned()
            .ok_or_else(|| FileError::NotFound(id.vpath().as_rootless_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.fonts.get(index)?.get()
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}
