use gpui::{Bounds, Pixels};
use project::ProjectEntryId;
use std::path::Path;
use std::sync::Arc;
use worktree::Entry;

const EXCLUDED_EXTENSIONS: &[&str] = &[
    // Images
    "png", "jpg", "jpeg", "gif", "ico", "svg", "webp", "bmp", "tiff", "tif",
    // Fonts
    "ttf", "otf", "woff", "woff2", "eot",
    // Media
    "mp3", "mp4", "wav", "ogg", "webm", "avi", "mov", "flac", "aac",
    // Archives
    "zip", "tar", "gz", "rar", "7z", "bz2", "xz",
    // Compiled/Binary
    "wasm", "so", "dylib", "dll", "exe", "o", "a", "lib", "pyc", "pyo", "class",
    // Data/Database
    "bin", "dat", "db", "sqlite", "sqlite3",
    // Documents (non-code)
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
    // IDE/Editor
    "vsix",
];

const EXCLUDED_DIRECTORIES: &[&str] = &[
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    "__pycache__",
    ".git",
    ".next",
    ".nuxt",
    ".cache",
    "coverage",
    ".idea",
    ".vscode",
    "venv",
    ".venv",
    "env",
    ".env",
];

fn should_include_entry(entry: &Entry) -> bool {
    let path_str = entry.path.as_unix_str();

    if entry.is_dir() {
        let dir_name = entry
            .path
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_default();
        return !EXCLUDED_DIRECTORIES.contains(&dir_name.as_str());
    }

    let extension = entry
        .path
        .extension()
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if EXCLUDED_EXTENSIONS.contains(&extension.as_str()) {
        return false;
    }

    // Also exclude files in excluded directories
    for excluded_dir in EXCLUDED_DIRECTORIES {
        if path_str.contains(&format!("{}/", excluded_dir)) {
            return false;
        }
    }

    true
}

/// Unique identifier for treemap nodes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeId {
    File(ProjectEntryId),
    #[allow(dead_code)]
    Directory(ProjectEntryId),
}

/// A node in the treemap (files only, flat layout)
#[derive(Clone, Debug)]
pub struct TreemapNode {
    pub id: NodeId,
    pub rel_path: Arc<str>,
    pub name: String,
    pub size: u64,
    #[allow(dead_code)]
    pub bounds: Bounds<Pixels>,
}

impl TreemapNode {
    pub fn from_entry(entry: &Entry) -> Self {
        let rel_path_str = entry.path.as_unix_str().to_string();
        let name = entry
            .path
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| rel_path_str.clone());

        Self {
            id: NodeId::File(entry.id),
            rel_path: rel_path_str.into(),
            name,
            size: entry.size,
            bounds: Bounds::default(),
        }
    }

    pub fn display_size(&self) -> u64 {
        self.size
    }
}

/// Builds a flat list of file nodes for the treemap (no directory nesting)
pub fn build_tree(entries: impl Iterator<Item = Entry>, _worktree_path: &Path) -> Vec<TreemapNode> {
    entries
        .filter(should_include_entry)
        .filter(|e| e.is_file())
        .map(|entry| TreemapNode::from_entry(&entry))
        .collect()
}


