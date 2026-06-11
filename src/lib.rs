#![doc = include_str!("../README.md")]

mod env;
mod error;
mod formatting;
mod typst;

pub use error::OgImageError;

use crate::env::var;
use crate::formatting::{serialize_bytes, serialize_number, serialize_optional_number};
use crate::typst::FontCache;
use ::typst::foundations::{Bytes, Dict, IntoValue};
use ::typst::syntax::FileId;
use reqwest::StatusCode;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
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

/// A downloaded avatar ready to be passed to the Typst compiler.
struct AvatarFile {
    /// The original avatar URL, used as the key in the avatar map.
    source: String,
    /// The local filename the template references (e.g. `avatar_0.png`).
    filename: String,
    /// The downloaded image bytes.
    bytes: Bytes,
}

/// Generator for creating OpenGraph images using the Typst typesetting system.
pub struct OgImageGenerator {
    typst_font_path: Option<PathBuf>,
    fonts: OnceLock<Arc<FontCache>>,
    #[cfg(feature = "oxipng")]
    optimize_png: bool,
}

impl OgImageGenerator {
    /// Creates a new `OgImageGenerator`.
    ///
    /// Fonts are discovered from the system. Use
    /// [`with_font_path()`](Self::with_font_path) to add a custom font directory.
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

    /// Creates a new `OgImageGenerator` using the `TYPST_FONT_PATH` environment variable.
    ///
    /// If the `TYPST_FONT_PATH` environment variable is set, uses that directory
    /// as an additional font path. Otherwise, only system fonts are used.
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
        let font_path = var("TYPST_FONT_PATH").map_err(OgImageError::EnvVarError)?;

        let mut generator = OgImageGenerator::default();

        if let Some(ref font_path) = font_path {
            debug!(font_path = %font_path, "Setting custom font path from environment");
            generator.typst_font_path = Some(PathBuf::from(font_path));
        } else {
            debug!("No custom font path specified, using Typst default font discovery");
        }

        Ok(generator)
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
        // Discard any font cache built from a previous path so the next
        // generation rebuilds it from the new path.
        self.fonts = OnceLock::new();

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

    /// Returns the shared font cache, building it on first access.
    ///
    /// Font discovery scans the system, so the result is cached for the lifetime
    /// of the generator and reused across image generations.
    fn fonts(&self) -> Arc<FontCache> {
        self.fonts
            .get_or_init(|| Arc::new(FontCache::load(self.typst_font_path.as_deref())))
            .clone()
    }

    /// Downloads the avatars referenced by the authors.
    ///
    /// URL-based avatars are downloaded into memory and their image format is
    /// detected to pick a file extension. Avatars that return 404 or have an
    /// unsupported format are skipped. Returns one [`AvatarFile`] per
    /// successfully downloaded avatar.
    #[instrument(skip(self, data), fields(krate.name = %data.name))]
    async fn process_avatars(
        &self,
        data: &OgImageData<'_>,
    ) -> Result<Vec<AvatarFile>, OgImageError> {
        let mut avatars = Vec::new();

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

                debug!(
                    author_name = %author.name,
                    avatar_url = %avatar,
                    size_bytes = bytes.len(),
                    "Avatar processed successfully"
                );

                avatars.push(AvatarFile {
                    source: avatar.to_string(),
                    filename,
                    bytes: Bytes::new(bytes),
                });
            }
        }

        Ok(avatars)
    }

    /// Generates an OpenGraph image using the provided data.
    ///
    /// This method downloads the referenced avatars, compiles the bundled
    /// template to PNG using the Typst library, and returns the resulting image
    /// as raw PNG bytes.
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

        // Process avatars - download URLs into memory
        let avatar_start_time = std::time::Instant::now();
        info!("Processing avatars");
        let avatars = self.process_avatars(&data).await?;
        let avatar_duration = avatar_start_time.elapsed();
        info!(
            avatar_count = avatars.len(),
            duration_ms = avatar_duration.as_millis(),
            "Avatar processing completed"
        );

        // Serialize data and avatar_map to JSON
        debug!("Serializing data and avatar map to JSON");
        let json_data =
            serde_json::to_string(&data).map_err(OgImageError::JsonSerializationError)?;

        let avatar_map: HashMap<&str, &str> = avatars
            .iter()
            .map(|avatar| (avatar.source.as_str(), avatar.filename.as_str()))
            .collect();
        let json_avatar_map =
            serde_json::to_string(&avatar_map).map_err(OgImageError::JsonSerializationError)?;

        let inputs: Dict = [("data", json_data), ("avatar_map", json_avatar_map)]
            .into_iter()
            .map(|(key, value)| (key.into(), value.into_value()))
            .collect();

        let avatar_files: HashMap<FileId, Bytes> = avatars
            .into_iter()
            .map(|avatar| {
                let path = format!("assets/{}", avatar.filename);
                (typst::file_id(&path), avatar.bytes)
            })
            .collect();

        // Compile the template to a PNG using the Typst library
        info!("Compiling template with Typst");
        let compilation_start_time = std::time::Instant::now();

        let fonts = self.fonts();

        let compiler = typst::Compiler::new(fonts, inputs, avatar_files);

        #[allow(unused_mut)]
        let mut png_data = tokio::task::spawn_blocking(move || compiler.compile_png())
            .await
            .map_err(|err| OgImageError::TypstTaskPanic(err.to_string()))??;

        let compilation_duration = compilation_start_time.elapsed();
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
    /// Creates a default `OgImageGenerator` using system font discovery.
    fn default() -> Self {
        Self {
            typst_font_path: None,
            fonts: OnceLock::new(),
            #[cfg(feature = "oxipng")]
            optimize_png: false,
        }
    }
}
