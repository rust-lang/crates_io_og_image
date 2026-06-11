//! Font discovery shared across compilations.

use std::path::Path;

use ::typst::text::FontBook;
use ::typst::utils::LazyHash;
use typst_kit::fonts::{FontSlot, Fonts};

/// The set of fonts available to the compiler.
///
/// Discovering fonts scans the system, so this is built once and shared across
/// compilations via an [`Arc`].
///
/// [`Arc`]: std::sync::Arc
pub struct FontCache {
    pub(crate) book: LazyHash<FontBook>,
    pub(crate) fonts: Vec<FontSlot>,
}

impl FontCache {
    /// Searches for fonts, optionally including an additional directory.
    ///
    /// Mirrors the CLI's font searcher: font directory, then system fonts, then
    /// the embedded default fonts.
    pub fn load(font_dir: Option<&Path>) -> Self {
        let mut searcher = Fonts::searcher();

        let fonts = match font_dir {
            Some(dir) => searcher.search_with([dir]),
            None => searcher.search(),
        };

        Self {
            book: LazyHash::new(fonts.book),
            fonts: fonts.fonts,
        }
    }
}
