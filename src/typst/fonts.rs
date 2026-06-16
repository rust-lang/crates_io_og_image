//! Font discovery shared across compilations.

use std::path::Path;

use ::typst::text::{Font, FontBook};
use ::typst::utils::LazyHash;
use typst_kit::fonts::{self, FontStore};

/// The set of fonts available to the compiler.
///
/// Discovering fonts scans the system, so this is built once and shared across
/// compilations via an [`Arc`].
///
/// [`Arc`]: std::sync::Arc
pub struct FontCache {
    store: FontStore,
}

impl FontCache {
    /// Searches for fonts, optionally including an additional directory.
    ///
    /// Mirrors the CLI's font searcher: font directory, then system fonts, then
    /// the embedded default fonts.
    pub fn load(font_dir: Option<&Path>) -> Self {
        let mut store = FontStore::new();

        if let Some(dir) = font_dir {
            store.extend(fonts::scan(dir));
        }
        store.extend(fonts::system());
        store.extend(fonts::embedded());

        Self { store }
    }

    pub(crate) fn book(&self) -> &LazyHash<FontBook> {
        self.store.book()
    }

    pub(crate) fn font(&self, index: usize) -> Option<Font> {
        self.store.font(index)
    }
}
