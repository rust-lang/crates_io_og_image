# crates_io_og_image

A Rust crate for generating Open Graph images for crates.io packages.

![Example OG Image](src/snapshots/crates_io_og_image__tests__generated_og_image.snap.png)

> [!NOTE]
> This crate is maintained by the crates.io team, primarily for use by crates.io and docs.rs, and not intended for external use. This crate may make major changes to its APIs or be deprecated without warning.

## Overview

`crates_io_og_image` is a specialized library for generating visually appealing Open Graph images for Rust crates. These images are designed to be displayed when crates.io links are shared on social media platforms, providing rich visual context about the crate including its name, description, authors, and key metrics.

The generated images include:

- Crate name and description
- Tags/keywords
- Author information with avatars (when available)
- Key metrics (releases, latest version, license, lines of code, size)
- Consistent crates.io branding

## Requirements

- The [Typst](https://typst.app/) CLI must be installed and available in your `PATH` (or configured via the `TYPST_PATH` environment variable).
- The [Fira Sans](https://github.com/mozilla/Fira) font must be installed on your system (or configured via the `TYPST_FONT_PATH` environment variable).
- The [Noto CJK](https://github.com/notofonts/noto-cjk) font may optionally be installed for CJK character support.
- The [Noto Color Emoji](https://github.com/googlefonts/noto-emoji/) font may optionally be installed for Emoji support.
- The [oxipng](https://github.com/shssoichiro/oxipng) CLI may optionally be installed for PNG file size optimization.

## Usage

### Basic Example

```rust
use crates_io_og_image::{OgImageData, OgImageGenerator, OgImageAuthorData, OgImageError};

#[tokio::main]
async fn main() -> Result<(), OgImageError> {
    // Create a generator instance
    let generator = OgImageGenerator::default();

    // Define the crate data
    let data = OgImageData {
        name: "example-crate",
        version: "1.2.3",
        description: Some("An example crate for testing OpenGraph image generation"),
        license: Some("MIT/Apache-2.0"),
        tags: &["example", "testing", "og-image"],
        authors: &[
            OgImageAuthorData::with_url(
                "Turbo87",
                "https://avatars.githubusercontent.com/u/141300",
            ),
        ],
        lines_of_code: Some(2000),
        crate_size: 75,
        releases: 5,
    };

    // Generate the image
    let temp_file = generator.generate(data).await?;

    // The temp_file contains the path to the generated PNG image
    println!("Image generated at: {}", temp_file.path().display());

    Ok(())
}
```

## Configuration

The following environment variables can be used to configure the image generation:

- `TYPST_PATH` - Path to the Typst CLI binary
- `TYPST_FONT_PATH` - Additional font directory
- `OXIPNG_PATH` - Path to the `oxipng` binary for PNG optimization

## Development

### Running Tests

```bash
cargo test
```

### Example

The crate includes an example that demonstrates how to generate an image:

```bash
cargo run --example test_generator
```

This will generate a test image in the current directory

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
