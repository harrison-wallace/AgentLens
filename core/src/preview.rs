//! Read-only file preview.
//!
//! Everything here is a read: the app never writes to a previewed file. Paths
//! go through `resolve_existing_in_workspace`, which resolves symlinks before
//! checking containment, so a preview can't be coaxed into reading outside the
//! open workspace even via a link planted inside it.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::paths::resolve_existing_in_workspace;
use crate::protocol::PreviewPayload;

/// Files above this are reported as `TooLarge` rather than shipped over IPC.
const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

/// How much of a file is inspected when deciding whether it's binary.
const SNIFF_BYTES: usize = 8_192;

/// Read `relative` for display. Never errors on file *content* — an
/// unreadable shape (too big, binary) is a payload variant, so the UI always
/// has something to render.
pub fn read(root: &Path, relative: &str) -> Result<PreviewPayload, String> {
    let target = resolve_existing_in_workspace(root, relative)?;
    let metadata = target
        .metadata()
        .map_err(|e| format!("failed to read file: {e}"))?;
    if metadata.is_dir() {
        return Err("path is a directory".to_string());
    }

    let size = metadata.len();
    let path = relative.to_string();
    if size > MAX_PREVIEW_BYTES {
        return Ok(PreviewPayload::TooLarge { path, size });
    }

    let bytes = std::fs::read(&target).map_err(|e| format!("failed to read file: {e}"))?;

    if let Some(mime) = image_mime(relative) {
        return Ok(PreviewPayload::Image {
            path,
            mime: mime.to_string(),
            base64: STANDARD.encode(&bytes),
        });
    }

    if looks_binary(&bytes) {
        return Ok(PreviewPayload::Binary { path, size });
    }

    match String::from_utf8(bytes) {
        Ok(text) => Ok(PreviewPayload::Text {
            language: language_for(relative).to_string(),
            path,
            text,
        }),
        Err(_) => Ok(PreviewPayload::Binary { path, size }),
    }
}

/// Open `relative` with the OS default application. The only outward-facing
/// action in the app, and still confined to the workspace.
pub fn resolve_for_open(root: &Path, relative: &str) -> Result<std::path::PathBuf, String> {
    resolve_existing_in_workspace(root, relative)
}

/// A NUL byte in the first few KB is the same heuristic git uses, and it
/// costs nothing next to the read we already did.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(SNIFF_BYTES).any(|byte| *byte == 0)
}

fn extension_of(relative: &str) -> String {
    Path::new(relative)
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The image MIME type for `relative`, or `None` if it isn't an image the
/// webview can render inline.
fn image_mime(relative: &str) -> Option<&'static str> {
    match extension_of(relative).as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "avif" => Some("image/avif"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

/// Shiki language id for `relative`. Falls back to `"text"`, which Shiki
/// renders unhighlighted rather than failing.
fn language_for(relative: &str) -> &'static str {
    let name = Path::new(relative)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Extension-less files that are still worth highlighting.
    match name.as_str() {
        "dockerfile" => return "docker",
        "makefile" => return "make",
        ".gitignore" | ".gitattributes" | ".dockerignore" | ".npmignore" => return "text",
        _ => {}
    }

    match extension_of(relative).as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "json" => "json",
        "jsonc" => "jsonc",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "sh" | "bash" | "zsh" => "shell",
        "ps1" => "powershell",
        "sql" => "sql",
        "xml" | "svg" => "xml",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "lua" => "lua",
        _ => "text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn language_for_maps_known_extensions_and_falls_back_to_text() {
        assert_eq!(language_for("src/main.rs"), "rust");
        assert_eq!(language_for("src/App.tsx"), "tsx");
        assert_eq!(language_for("README.md"), "markdown");
        assert_eq!(language_for("Cargo.TOML"), "toml");
        assert_eq!(language_for("Dockerfile"), "docker");
        assert_eq!(language_for("notes.unknownext"), "text");
        assert_eq!(language_for("LICENSE"), "text");
    }

    #[test]
    fn image_mime_recognizes_images_only() {
        assert_eq!(image_mime("logo.PNG"), Some("image/png"));
        assert_eq!(image_mime("photo.jpeg"), Some("image/jpeg"));
        assert_eq!(image_mime("main.rs"), None);
    }

    #[test]
    fn looks_binary_only_on_nul_bytes() {
        assert!(!looks_binary(b"fn main() {}\n"));
        assert!(looks_binary(b"MZ\x00\x00binary"));
        // A NUL past the sniff window is not inspected, by design.
        let mut late = vec![b'a'; SNIFF_BYTES];
        late.push(0);
        assert!(!looks_binary(&late));
    }

    #[test]
    fn reads_text_with_a_language() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let payload = read(dir.path(), "main.rs").unwrap();
        assert_eq!(
            payload,
            PreviewPayload::Text {
                path: "main.rs".to_string(),
                text: "fn main() {}".to_string(),
                language: "rust".to_string(),
            }
        );
    }

    #[test]
    fn reads_image_as_base64() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pixel.png"), [1u8, 2, 3]).unwrap();

        let payload = read(dir.path(), "pixel.png").unwrap();
        assert_eq!(
            payload,
            PreviewPayload::Image {
                path: "pixel.png".to_string(),
                mime: "image/png".to_string(),
                base64: STANDARD.encode([1u8, 2, 3]),
            }
        );
    }

    #[test]
    fn reports_binary_files_rather_than_mangling_them() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), [0u8, 159, 146, 150]).unwrap();

        let payload = read(dir.path(), "a.bin").unwrap();
        assert_eq!(
            payload,
            PreviewPayload::Binary {
                path: "a.bin".to_string(),
                size: 4,
            }
        );
    }

    #[test]
    fn reports_oversized_files_without_reading_them() {
        let dir = tempfile::tempdir().unwrap();
        let size = MAX_PREVIEW_BYTES + 1;
        fs::write(dir.path().join("big.txt"), vec![b'a'; size as usize]).unwrap();

        let payload = read(dir.path(), "big.txt").unwrap();
        assert_eq!(
            payload,
            PreviewPayload::TooLarge {
                path: "big.txt".to_string(),
                size,
            }
        );
    }

    #[test]
    fn refuses_to_read_outside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path(), "../secret").is_err());
        assert!(resolve_for_open(dir.path(), "../secret").is_err());
    }

    #[test]
    fn refuses_to_read_through_a_symlink_out_of_the_workspace() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "classified").unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, root.join("link.txt")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&secret, root.join("link.txt")).unwrap();

        assert!(read(&root, "link.txt").is_err());
        assert!(resolve_for_open(&root, "link.txt").is_err());
    }
}
