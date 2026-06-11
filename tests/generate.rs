use crates_io_og_image::{OgImageAuthorData, OgImageData, OgImageGenerator};
use mockito::{Server, ServerGuard};
use tracing::dispatcher::DefaultGuard;
use tracing::{Level, subscriber};
use tracing_subscriber::fmt;

fn init_tracing() -> DefaultGuard {
    let subscriber = fmt()
        .compact()
        .with_max_level(Level::DEBUG)
        .with_test_writer()
        .finish();

    subscriber::set_default(subscriber)
}

async fn create_mock_avatar_server() -> ServerGuard {
    let mut server = Server::new_async().await;

    // Mock for successful PNG avatar download
    server
        .mock("GET", "/test-avatar.png")
        .with_status(200)
        .with_header("content-type", "image/png")
        .with_body(include_bytes!("../template/assets/test-avatar.png"))
        .create();

    // Mock for JPEG avatar download
    server
        .mock("GET", "/test-avatar.jpg")
        .with_status(200)
        .with_header("content-type", "image/jpeg")
        .with_body(include_bytes!("../template/assets/test-avatar.jpg"))
        .create();

    // Mock for unsupported file type (WebP)
    server
        .mock("GET", "/test-avatar.webp")
        .with_status(200)
        .with_header("content-type", "image/webp")
        .with_body(include_bytes!("../template/assets/test-avatar.webp"))
        .create();

    // Mock for 404 avatar download
    server
        .mock("GET", "/missing-avatar.png")
        .with_status(404)
        .with_header("content-type", "text/plain")
        .with_body("Not Found")
        .create();

    server
}

const fn author(name: &str) -> OgImageAuthorData<'_> {
    OgImageAuthorData::new(name, None)
}

fn author_with_avatar(name: &str, url: String) -> OgImageAuthorData<'_> {
    OgImageAuthorData::with_url(name, url)
}

fn create_minimal_test_data() -> OgImageData<'static> {
    static AUTHORS: &[OgImageAuthorData<'_>] = &[author("author")];

    OgImageData {
        name: "minimal-crate",
        version: "1.0.0",
        description: None,
        license: None,
        tags: &[],
        authors: AUTHORS,
        lines_of_code: None,
        crate_size: 10000,
        releases: 1,
    }
}

fn create_escaping_authors(server_url: &str) -> Vec<OgImageAuthorData<'_>> {
    vec![
        author_with_avatar(
            "author \"with quotes\"",
            format!("{server_url}/test-avatar.png"),
        ),
        author("author\\with\\backslashes"),
        author("author#with#hashes"),
    ]
}

fn create_escaping_test_data<'a>(authors: &'a [OgImageAuthorData<'a>]) -> OgImageData<'a> {
    OgImageData {
        name: "crate-with-\"quotes\"",
        version: "1.0.0-\"beta\"",
        description: Some(
            "A crate with \"quotes\", \\ backslashes, and other special chars: #[]{}()",
        ),
        license: Some("MIT OR \"Apache-2.0\""),
        tags: &[
            "tag-with-\"quotes\"",
            "tag\\with\\backslashes",
            "tag#with#symbols",
        ],
        authors,
        lines_of_code: Some(42),
        crate_size: 256256,
        releases: 5,
    }
}

fn create_overflow_authors(server_url: &str) -> Vec<OgImageAuthorData<'_>> {
    vec![
        author_with_avatar("alice-wonderland", format!("{server_url}/test-avatar.png")),
        author("bob-the-builder"),
        author_with_avatar("charlie-brown", format!("{server_url}/test-avatar.jpg")),
        author("diana-prince"),
        author_with_avatar(
            "edward-scissorhands",
            format!("{server_url}/test-avatar.png"),
        ),
        author("fiona-apple"),
        author_with_avatar(
            "george-washington",
            format!("{server_url}/test-avatar.webp"),
        ),
        author_with_avatar("helen-keller", format!("{server_url}/test-avatar.jpg")),
        author("isaac-newton"),
        author("jane-doe"),
    ]
}

fn create_overflow_test_data<'a>(authors: &'a [OgImageAuthorData<'a>]) -> OgImageData<'a> {
    OgImageData {
        name: "super-long-crate-name-for-testing-overflow-behavior",
        version: "2.1.0-beta.1+build.12345",
        description: Some(
            "This is an extremely long description that tests how the layout handles descriptions that might wrap to multiple lines or overflow the available space in the OpenGraph image template design. This is an extremely long description that tests how the layout handles descriptions that might wrap to multiple lines or overflow the available space in the OpenGraph image template design.",
        ),
        license: Some("MIT/Apache-2.0/ISC/BSD-3-Clause"),
        tags: &[
            "web-framework",
            "async-runtime",
            "database-orm",
            "serialization",
            "networking",
        ],
        authors,
        lines_of_code: Some(147000),
        crate_size: 2847123,
        releases: 1432,
    }
}

fn create_simple_test_data() -> OgImageData<'static> {
    static AUTHORS: &[OgImageAuthorData<'_>] = &[author("test-user")];

    OgImageData {
        name: "test-crate",
        version: "1.0.0",
        description: Some("A test crate for OpenGraph image generation"),
        license: Some("MIT/Apache-2.0"),
        tags: &["testing", "og-image"],
        authors: AUTHORS,
        lines_of_code: Some(1000),
        crate_size: 42012,
        releases: 1,
    }
}

async fn generate_image(data: OgImageData<'_>) -> Vec<u8> {
    let generator =
        OgImageGenerator::from_environment().expect("Failed to create OgImageGenerator");

    // Snapshots are optimized PNGs, so the tests must opt into optimization.
    // The `cfg` keeps the test binary compiling under `--no-default-features`.
    #[cfg(feature = "oxipng")]
    let generator = generator.with_oxipng();

    generator
        .generate(data)
        .await
        .expect("Failed to generate image")
}

#[tokio::test]
async fn test_generate_og_image_snapshot() {
    let _guard = init_tracing();
    let data = create_simple_test_data();

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("generated_og_image.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_overflow_snapshot() {
    let _guard = init_tracing();

    let server = create_mock_avatar_server().await;
    let server_url = server.url();

    let authors = create_overflow_authors(&server_url);
    let data = create_overflow_test_data(&authors);

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("generated_og_image_overflow.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_minimal_snapshot() {
    let _guard = init_tracing();
    let data = create_minimal_test_data();

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("generated_og_image_minimal.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_escaping_snapshot() {
    let _guard = init_tracing();

    let server = create_mock_avatar_server().await;
    let server_url = server.url();

    let authors = create_escaping_authors(&server_url);
    let data = create_escaping_test_data(&authors);

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("generated_og_image_escaping.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_with_404_avatar() {
    let _guard = init_tracing();

    let server = create_mock_avatar_server().await;
    let server_url = server.url();

    // Create test data with a 404 avatar URL - should skip the avatar gracefully
    let authors = vec![author_with_avatar(
        "test-user",
        format!("{server_url}/missing-avatar.png"),
    )];
    let data = OgImageData {
        name: "test-crate-404",
        version: "1.0.0",
        description: Some("A test crate with 404 avatar"),
        license: Some("MIT"),
        tags: &["testing"],
        authors: &authors,
        lines_of_code: Some(1000),
        crate_size: 42012,
        releases: 1,
    };

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("404-avatar.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_unicode_truncation() {
    let _guard = init_tracing();

    // Test case that reproduces the Unicode truncation bug from issue #11524
    // Uses the exact description from "adder-codec-rs" crate which contains
    // multibyte Unicode characters (Δ) that cause string slicing to fail
    static AUTHORS: &[OgImageAuthorData<'_>] = &[author("adder-codec-rs-author")];

    let data = OgImageData {
        name: "adder-codec-rs",
        version: "1.0.0",
        description: Some(
            "Encoder/transcoder/decoder for raw and compressed ADΔER (Address, Decimation, Δt Event Representation) streams. Includes a transcoder for casting either framed or event video into an ADΔER representation in a manner which preserves the temporal resolution of the source. This is a very long description that should trigger text truncation to test the Unicode character boundary issue when the text is too long to fit in the available space. Adding even more text with Unicode characters like ADΔER and Δt to ensure we hit the problematic slice operation at character boundaries.",
        ),
        license: Some("MIT"),
        tags: &["codec", "adder", "event-representation"],
        authors: AUTHORS,
        lines_of_code: Some(5000),
        crate_size: 128000,
        releases: 3,
    };

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("unicode-truncation.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_asian_text_description() {
    let _guard = init_tracing();

    static AUTHORS: &[OgImageAuthorData<'_>] = &[author("アジア開発者")];

    let data = OgImageData {
        name: "internationalization-crate",
        version: "1.0.0",
        description: Some(
            "这是一个支持多种亚洲语言的包。包含中文（简体和繁體）、日本語、한국어支持。本包装提供了强大的国际化功能，能够处理各种复杂的文字系统和字符编码。特别适合需要处理多语言文本的应用程序。機能には文字列処理、エンコーディング変換、로케일 지원이 포함됩니다。这个描述很长，用来测试文本截断功能是否能正确处理多字节字符边界问题。",
        ),
        license: Some("MIT"),
        tags: &[
            "internationalization",
            "multilang",
            "unicode",
            "text-processing",
        ],
        authors: AUTHORS,
        lines_of_code: Some(8000),
        crate_size: 256000,
        releases: 12,
    };

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("asian-text-description.png", image_data);
}

#[tokio::test]
async fn test_generate_og_image_whitespace_handling() {
    let _guard = init_tracing();

    static AUTHORS: &[OgImageAuthorData<'_>] = &[author("test-author")];

    let data = OgImageData {
        name: "whitespace-test-crate",
        version: "1.0.0",
        description: Some(
            "This description contains various whitespace characters:\n\nNewlines (\\n),\r\nWindows line endings (\\r\\n),\r\rCarriage returns (\\r),\t\tTabs (\\t),    Multiple spaces,\u{00A0}Non-breaking spaces,\u{2000}\u{2001}\u{2002}Unicode spaces, and     mixed    \t\n\r  whitespace   \t\n  patterns.    This tests how the template handles all types of whitespace normalization and rendering behavior.",
        ),
        license: Some("MIT"),
        tags: &["whitespace", "testing", "normalization"],
        authors: AUTHORS,
        lines_of_code: Some(1500),
        crate_size: 50000,
        releases: 2,
    };

    let image_data = generate_image(data).await;
    insta::assert_binary_snapshot!("whitespace-handling.png", image_data);
}
