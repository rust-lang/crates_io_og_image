#![doc = include_str!("../README.md")]

mod env;
mod error;
mod formatting;

pub use error::OgImageError;

use crate::env::var;
use crate::formatting::{serialize_bytes, serialize_number, serialize_optional_number};
use reqwest::StatusCode;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, error, info, instrument, warn};

/// Data structure containing information needed to generate an OpenGraph image
/// for a crates.io crate.
#[derive(Debug, Clone, Serialize)]
pub struct OgImageData<'a> {
    /// The crate name
    pub name: &'a str,
    /// Latest version string (e.g., "1.0.210")
    pub version: &'a str,
    /// Crate description text
    pub description: Option<&'a str>,
    /// License information (e.g., "MIT/Apache-2.0")
    pub license: Option<&'a str>,
    /// Keywords/categories for the crate
    pub tags: &'a [&'a str],
    /// Author information
    pub authors: &'a [OgImageAuthorData<'a>],
    /// Source lines of code count (optional)
    #[serde(serialize_with = "serialize_optional_number")]
    pub lines_of_code: Option<u32>,
    /// Package size in bytes
    #[serde(serialize_with = "serialize_bytes")]
    pub crate_size: u32,
    /// Total number of releases
    #[serde(serialize_with = "serialize_number")]
    pub releases: u32,
}

/// Author information for OpenGraph image generation
#[derive(Debug, Clone, Serialize)]
pub struct OgImageAuthorData<'a> {
    /// Author username/name
    pub name: &'a str,
    /// Optional avatar URL
    pub avatar: Option<Cow<'a, str>>,
}

impl<'a> OgImageAuthorData<'a> {
    /// Creates a new `OgImageAuthorData` with the specified name and optional avatar.
    pub const fn new(name: &'a str, avatar: Option<Cow<'a, str>>) -> Self {
        Self { name, avatar }
    }

    /// Creates a new `OgImageAuthorData` with a URL-based avatar.
    pub fn with_url(name: &'a str, url: impl Into<Cow<'a, str>>) -> Self {
        Self::new(name, Some(url.into()))
    }
}

/// Generator for creating OpenGraph images using the Typst typesetting system.
///
/// This struct manages the path to the Typst binary and provides methods for
/// generating PNG images from a Typst template.
pub struct OgImageGenerator {
    typst_binary_path: PathBuf,
    typst_font_path: Option<PathBuf>,
    #[cfg(feature = "oxipng")]
    optimize_png: bool,
}

impl OgImageGenerator {
    /// Creates a new `OgImageGenerator` with the default Typst binary path.
    ///
    /// Uses "typst" as the default binary path, assuming it is available in
    /// PATH. Use [`with_typst_path()`](Self::with_typst_path) to customize the
    /// binary path.
    ///
    /// # Examples
    ///
    /// ```
    /// use crates_io_og_image::OgImageGenerator;
    ///
    /// let generator = OgImageGenerator::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Detects the image format from the first few bytes using magic numbers.
    ///
    /// Returns the appropriate file extension for supported formats:
    /// - PNG: returns "png"
    /// - JPEG: returns "jpg"
    /// - Unsupported formats: returns None
    fn detect_image_format(bytes: &[u8]) -> Option<&'static str> {
        // PNG magic number: 89 50 4E 47 0D 0A 1A 0A
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some("png");
        }

        // JPEG magic number: FF D8 FF
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some("jpg");
        }

        None
    }

    /// Creates a new `OgImageGenerator` using the `TYPST_PATH` environment variable.
    ///
    /// If the `TYPST_PATH` environment variable is set, uses that path.
    /// Otherwise, falls back to the default behavior (assumes "typst" is in PATH).
    ///
    /// # Examples
    ///
    /// ```
    /// use crates_io_og_image::OgImageGenerator;
    ///
    /// let generator = OgImageGenerator::from_environment()?;
    /// # Ok::<(), crates_io_og_image::OgImageError>(())
    /// ```
    #[instrument]
    pub fn from_environment() -> Result<Self, OgImageError> {
        let typst_path = var("TYPST_PATH").map_err(OgImageError::EnvVarError)?;
        let font_path = var("TYPST_FONT_PATH").map_err(OgImageError::EnvVarError)?;

        let mut generator = OgImageGenerator::default();

        if let Some(ref path) = typst_path {
            debug!(typst_path = %path, "Using custom Typst binary path from environment");
            generator.typst_binary_path = PathBuf::from(path);
        } else {
            debug!("Using default Typst binary path (assumes 'typst' in PATH)");
        };

        if let Some(ref font_path) = font_path {
            debug!(font_path = %font_path, "Setting custom font path from environment");
            generator.typst_font_path = Some(PathBuf::from(font_path));
        } else {
            debug!("No custom font path specified, using Typst default font discovery");
        }

        Ok(generator)
    }

    /// Sets the Typst binary path for the generator.
    ///
    /// This allows specifying a custom path to the Typst binary.
    /// If not set, defaults to "typst" which assumes the binary is available in PATH.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use crates_io_og_image::OgImageGenerator;
    ///
    /// let generator = OgImageGenerator::default()
    ///     .with_typst_path(PathBuf::from("/usr/local/bin/typst"));
    /// ```
    pub fn with_typst_path(mut self, typst_path: PathBuf) -> Self {
        self.typst_binary_path = typst_path;
        self
    }

    /// Sets the font path for the Typst compiler.
    ///
    /// This allows specifying a custom directory where Typst will look for fonts
    /// during compilation in addition to the default system font discovery.
    /// If not set, Typst will use only its default font discovery.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use crates_io_og_image::OgImageGenerator;
    ///
    /// let generator = OgImageGenerator::default()
    ///     .with_font_path(PathBuf::from("/usr/share/fonts"));
    /// ```
    pub fn with_font_path(mut self, font_path: PathBuf) -> Self {
        self.typst_font_path = Some(font_path);
        self
    }

    /// Enables in-process PNG optimization using the `oxipng` library.
    ///
    /// Optimization is disabled by default. Failures are logged and never abort
    /// image generation, so optimization remains best-effort.
    ///
    /// # Examples
    ///
    /// ```
    /// use crates_io_og_image::OgImageGenerator;
    ///
    /// let generator = OgImageGenerator::default().with_oxipng();
    /// ```
    #[cfg(feature = "oxipng")]
    pub fn with_oxipng(mut self) -> Self {
        self.optimize_png = true;
        self
    }

    /// Processes avatars by downloading URLs and copying assets to the assets directory.
    ///
    /// This method handles both asset-based avatars (which are copied from the bundled assets)
    /// and URL-based avatars (which are downloaded from the internet).
    /// Returns a mapping from avatar source to the local filename.
    #[instrument(skip(self, data), fields(krate.name = %data.name))]
    async fn process_avatars<'a>(
        &self,
        data: &'a OgImageData<'_>,
        assets_dir: &Path,
    ) -> Result<HashMap<&'a str, String>, OgImageError> {
        let mut avatar_map = HashMap::new();

        let client = reqwest::Client::new();
        for (index, author) in data.authors.iter().enumerate() {
            if let Some(avatar) = &author.avatar {
                debug!(
                    author_name = %author.name,
                    avatar_url = %avatar,
                    "Processing avatar for author {}", author.name
                );

                // Download the avatar from the URL
                debug!(url = %avatar, "Downloading avatar from URL: {avatar}");
                let response = client.get(avatar.as_ref()).send().await.map_err(|err| {
                    OgImageError::AvatarDownloadError {
                        url: avatar.to_string(),
                        source: err,
                    }
                })?;

                let status = response.status();
                if status == StatusCode::NOT_FOUND {
                    warn!(url = %avatar, "Avatar URL returned 404 Not Found");
                    continue; // Skip this avatar if not found
                }

                if let Err(err) = response.error_for_status_ref() {
                    return Err(OgImageError::AvatarDownloadError {
                        url: avatar.to_string(),
                        source: err,
                    });
                }

                let content_length = response.content_length();
                debug!(
                    url = %avatar,
                    content_length = ?content_length,
                    status = %response.status(),
                    "Avatar download response received"
                );

                let bytes = response.bytes().await;
                let bytes = bytes.map_err(|err| {
                    error!(url = %avatar, error = %err, "Failed to read avatar response bytes");
                    OgImageError::AvatarDownloadError {
                        url: (*avatar).to_string(),
                        source: err,
                    }
                })?;

                debug!(url = %avatar, size_bytes = bytes.len(), "Avatar downloaded successfully");

                // Detect the image format and determine the appropriate file extension
                let Some(extension) = Self::detect_image_format(&bytes) else {
                    // Format not supported, log warning with first 20 bytes for debugging
                    let debug_bytes = &bytes[..bytes.len().min(20)];
                    let hex_bytes = debug_bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ");

                    warn!("Unsupported avatar format at {avatar}, first 20 bytes: {hex_bytes}");

                    // Skip this avatar and continue with the next one
                    continue;
                };

                let filename = format!("avatar_{index}.{extension}");
                let avatar_path = assets_dir.join(&filename);

                debug!(
                    author_name = %author.name,
                    avatar_url = %avatar,
                    avatar_path = %avatar_path.display(),
                    "Writing avatar file with detected format"
                );

                // Write the bytes to the avatar file
                fs::write(&avatar_path, &bytes).await.map_err(|err| {
                    OgImageError::AvatarWriteError {
                        path: avatar_path.clone(),
                        source: err,
                    }
                })?;

                debug!(
                    author_name = %author.name,
                    path = %avatar_path.display(),
                    size_bytes = bytes.len(),
                    "Avatar processed and written successfully"
                );

                // Store the mapping from the avatar source to the numbered filename
                avatar_map.insert(avatar.as_ref(), filename);
            }
        }

        Ok(avatar_map)
    }

    /// Generates an OpenGraph image using the provided data.
    ///
    /// This method creates a temporary directory with all the necessary files
    /// to create the OpenGraph image, compiles it to PNG using the Typst
    /// binary, and returns the resulting image as raw PNG bytes.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crates_io_og_image::{OgImageGenerator, OgImageData, OgImageAuthorData, OgImageError};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), OgImageError> {
    /// let generator = OgImageGenerator::default();
    /// let data = OgImageData {
    ///     name: "my-crate",
    ///     version: "1.0.0",
    ///     description: Some("A sample crate"),
    ///     license: Some("MIT"),
    ///     tags: &["web", "api"],
    ///     authors: &[OgImageAuthorData { name: "user", avatar: None }],
    ///     lines_of_code: Some(5000),
    ///     crate_size: 100,
    ///     releases: 10,
    /// };
    /// let image = generator.generate(data).await?;
    /// println!("Generated image: {} bytes", image.len());
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, data), fields(
        crate.name = %data.name,
        crate.version = %data.version,
        author_count = data.authors.len(),
    ))]
    pub async fn generate(&self, data: OgImageData<'_>) -> Result<Vec<u8>, OgImageError> {
        let start_time = std::time::Instant::now();
        info!("Starting OpenGraph image generation");

        // Create a temporary folder
        let temp_dir = tempfile::tempdir().map_err(OgImageError::TempDirError)?;
        debug!(temp_dir = %temp_dir.path().display(), "Created temporary directory");

        // Create assets directory and copy logo and icons
        let assets_dir = temp_dir.path().join("assets");
        debug!(assets_dir = %assets_dir.display(), "Creating assets directory");
        fs::create_dir(&assets_dir).await?;

        debug!("Copying bundled assets to temporary directory");
        let cargo_logo = include_bytes!("../template/assets/cargo.png");
        fs::write(assets_dir.join("cargo.png"), cargo_logo).await?;
        let rust_logo_svg = include_bytes!("../template/assets/rust-logo.svg");
        fs::write(assets_dir.join("rust-logo.svg"), rust_logo_svg).await?;

        // Copy SVG icons
        debug!("Copying SVG icon assets");
        let code_branch_svg = include_bytes!("../template/assets/code-branch.svg");
        fs::write(assets_dir.join("code-branch.svg"), code_branch_svg).await?;
        let code_svg = include_bytes!("../template/assets/code.svg");
        fs::write(assets_dir.join("code.svg"), code_svg).await?;
        let scale_balanced_svg = include_bytes!("../template/assets/scale-balanced.svg");
        fs::write(assets_dir.join("scale-balanced.svg"), scale_balanced_svg).await?;
        let tag_svg = include_bytes!("../template/assets/tag.svg");
        fs::write(assets_dir.join("tag.svg"), tag_svg).await?;
        let weight_hanging_svg = include_bytes!("../template/assets/weight-hanging.svg");
        fs::write(assets_dir.join("weight-hanging.svg"), weight_hanging_svg).await?;

        // Process avatars - download URLs and copy assets
        let avatar_start_time = std::time::Instant::now();
        info!("Processing avatars");
        let avatar_map = self.process_avatars(&data, &assets_dir).await?;
        let avatar_duration = avatar_start_time.elapsed();
        info!(
            avatar_count = avatar_map.len(),
            duration_ms = avatar_duration.as_millis(),
            "Avatar processing completed"
        );

        // Copy the static Typst template file
        let template_content = include_str!("../template/og-image.typ");
        let typ_file_path = temp_dir.path().join("og-image.typ");
        debug!(template_path = %typ_file_path.display(), "Copying Typst template");
        fs::write(&typ_file_path, template_content).await?;

        // Serialize data and avatar_map to JSON
        debug!("Serializing data and avatar map to JSON");
        let json_data =
            serde_json::to_string(&data).map_err(OgImageError::JsonSerializationError)?;

        let json_avatar_map =
            serde_json::to_string(&avatar_map).map_err(OgImageError::JsonSerializationError)?;

        // Run typst compile command with input data
        info!("Running Typst compilation command");
        let mut command = Command::new(&self.typst_binary_path);
        command.arg("compile").arg("--format").arg("png");

        // Pass in the data and avatar map as JSON inputs
        let input = format!("data={json_data}");
        command.arg("--input").arg(input);
        let input = format!("avatar_map={json_avatar_map}");
        command.arg("--input").arg(input);

        // Pass in the font path if specified
        if let Some(font_path) = &self.typst_font_path {
            debug!(font_path = %font_path.display(), "Using custom font path");
            command.arg("--font-path").arg(font_path);
        } else {
            debug!("Using only system fonts");
        }

        // Pass input file path and compile to stdout
        command.arg(&typ_file_path).arg("-");

        // Clear environment variables to avoid leaking sensitive data
        command.env_clear();

        // Preserve environment variables needed for font discovery
        if let Ok(path) = std::env::var("PATH") {
            command.env("PATH", path);
        }
        if let Ok(home) = std::env::var("HOME") {
            command.env("HOME", home);
        }

        let compilation_start_time = std::time::Instant::now();
        let output = command.output().await;
        let output = output.map_err(OgImageError::TypstNotFound)?;
        let compilation_duration = compilation_start_time.elapsed();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            error!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                stdout = %stdout,
                duration_ms = compilation_duration.as_millis(),
                "Typst compilation failed"
            );
            return Err(OgImageError::TypstCompilationError {
                stderr,
                stdout,
                exit_code: output.status.code(),
            });
        }

        #[allow(unused_mut)]
        let mut png_data = output.stdout;
        let output_size_bytes = png_data.len();

        debug!(
            duration_ms = compilation_duration.as_millis(),
            output_size_bytes, "Typst compilation completed successfully"
        );

        // After successful Typst compilation, optimize the PNG
        #[cfg(feature = "oxipng")]
        if self.optimize_png {
            png_data = Self::optimize_png(png_data).await;
        }

        let output_size_bytes = png_data.len();
        let duration = start_time.elapsed();
        info!(
            duration_ms = duration.as_millis(),
            output_size_bytes, "OpenGraph image generation completed successfully"
        );
        Ok(png_data)
    }

    /// Optimizes PNG image data in memory using the `oxipng` library.
    ///
    /// This method attempts to reduce the size of a PNG using lossless compression
    /// and returns the optimized data. On failure it returns the original data
    /// unchanged. All errors are handled internally and logged as warnings to
    /// ensure PNG optimization is truly optional.
    #[cfg(feature = "oxipng")]
    async fn optimize_png(data: Vec<u8>) -> Vec<u8> {
        use oxipng::{Options, StripChunks};
        use std::sync::Arc;

        let input_size = data.len();
        debug!("Starting PNG optimization");

        let start_time = std::time::Instant::now();

        let data = Arc::new(data);
        let task_data = data.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut options = Options::from_preset(2);
            options.strip = StripChunks::Safe;

            oxipng::optimize_from_memory(&task_data, &options)
        })
        .await;

        let duration = start_time.elapsed();

        match result {
            Ok(Ok(optimized)) => {
                let output_size = optimized.len();
                debug!(
                    duration_ms = duration.as_millis(),
                    "PNG optimization reduced image from {input_size} to {output_size} bytes"
                );
                optimized
            }
            Ok(Err(err)) => {
                warn!(
                    error = %err,
                    duration_ms = duration.as_millis(),
                    "PNG optimization failed, continuing with unoptimized image"
                );
                Arc::unwrap_or_clone(data)
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "PNG optimization task panicked, continuing with unoptimized image"
                );
                Arc::unwrap_or_clone(data)
            }
        }
    }
}

impl Default for OgImageGenerator {
    /// Creates a default `OgImageGenerator` with the default Typst binary path.
    ///
    /// Uses "typst" as the default binary path, assuming it is available in PATH.
    fn default() -> Self {
        Self {
            typst_binary_path: PathBuf::from("typst"),
            typst_font_path: None,
            #[cfg(feature = "oxipng")]
            optimize_png: false,
        }
    }
}
