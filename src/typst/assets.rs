use std::collections::HashMap;
use std::sync::LazyLock;

use ::typst::foundations::Bytes;
use ::typst::syntax::{FileId, Source};

use super::file_id;

/// The parsed main template source.
pub static MAIN_SOURCE: LazyLock<Source> = LazyLock::new(|| {
    let text = include_str!("../../template/og-image.typ").to_string();
    Source::new(file_id("og-image.typ"), text)
});

/// Pairs an asset's virtual path with its embedded bytes from a single filename.
macro_rules! asset {
    ($name:literal) => {
        (
            concat!("assets/", $name),
            include_bytes!(concat!("../../template/assets/", $name)),
        )
    };
}

/// The bundled logo and icon assets.
const RAW_ASSETS: &[(&str, &[u8])] = &[
    asset!("cargo.png"),
    asset!("rust-logo.svg"),
    asset!("code-branch.svg"),
    asset!("code.svg"),
    asset!("scale-balanced.svg"),
    asset!("tag.svg"),
    asset!("weight-hanging.svg"),
];

/// The bundled assets as an in-memory file set.
pub static ASSETS: LazyLock<HashMap<FileId, Bytes>> = LazyLock::new(|| {
    RAW_ASSETS
        .iter()
        .map(|(path, bytes)| (file_id(path), Bytes::new(*bytes)))
        .collect()
});
