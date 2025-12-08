use gpui::{Bounds, Pixels};
use project::ProjectEntryId;
use std::collections::HashMap;
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

/// A node in the treemap (hierarchical)
#[derive(Clone, Debug)]
pub struct TreemapNode {
    pub id: NodeId,
    pub rel_path: Arc<str>,
    pub name: String,
    pub size: u64,
    pub children: Vec<TreemapNode>,
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
            id: if entry.is_file() {
                NodeId::File(entry.id)
            } else {
                NodeId::Directory(entry.id)
            },
            rel_path: rel_path_str.into(),
            name,
            size: entry.size,
            children: Vec::new(),
            bounds: Bounds::default(),
        }
    }

    pub fn is_directory(&self) -> bool {
        matches!(self.id, NodeId::Directory(_))
    }

    pub fn compute_aggregate_size(&mut self) {
        if !self.children.is_empty() {
            self.size = self.children.iter().map(|c| c.size).sum();
        }
    }

    pub fn display_size(&self) -> u64 {
        self.size
    }
}

/// Builds hierarchical tree from flat file entries
pub fn build_tree(entries: impl Iterator<Item = Entry>, _worktree_path: &Path) -> Vec<TreemapNode> {
    use std::collections::HashSet;

    let mut files_by_parent: HashMap<String, HashMap<String, Entry>> = HashMap::new();
    let mut directories: HashMap<String, Entry> = HashMap::new();
    let mut children_by_parent: HashMap<String, HashSet<String>> = HashMap::new();
    let mut all_dir_paths: HashSet<String> = HashSet::new();

    for entry in entries.filter(should_include_entry) {
        let path_str = entry.path.as_unix_str().to_string();
        if entry.is_dir() {
            let parent = entry
                .path
                .parent()
                .map(|p| p.as_unix_str().to_string())
                .unwrap_or_default();
            children_by_parent
                .entry(parent)
                .or_default()
                .insert(path_str.clone());
            all_dir_paths.insert(path_str.clone());
            directories.insert(path_str, entry);
        } else {
            let parent = entry
                .path
                .parent()
                .map(|p| p.as_unix_str().to_string())
                .unwrap_or_default();
            files_by_parent
                .entry(parent)
                .or_default()
                .insert(path_str, entry);
        }
    }

    // Sort directories by depth (deepest first)
    let mut all_dir_paths: Vec<String> = all_dir_paths.into_iter().collect();
    all_dir_paths.sort_by(|a, b| {
        let depth_a = a.matches('/').count();
        let depth_b = b.matches('/').count();
        depth_b.cmp(&depth_a)
    });

    // Build nodes bottom-up
    let mut built_nodes: HashMap<String, Vec<TreemapNode>> = HashMap::new();

    for dir_path in &all_dir_paths {
        let mut children = Vec::new();

        if let Some(files) = files_by_parent.get(dir_path) {
            for entry in files.values() {
                children.push(TreemapNode::from_entry(entry));
            }
        }

        if let Some(child_dir_paths) = children_by_parent.get(dir_path) {
            for child_path in child_dir_paths {
                if let Some(mut child_nodes) = built_nodes.remove(child_path) {
                    children.append(&mut child_nodes);
                }
            }
        }

        if let Some(dir_entry) = directories.get(dir_path) {
            let mut dir_node = TreemapNode::from_entry(dir_entry);
            dir_node.children = children;
            dir_node.compute_aggregate_size();
            if dir_node.size > 0 {
                let parent = dir_entry
                    .path
                    .parent()
                    .map(|p| p.as_unix_str().to_string())
                    .unwrap_or_default();
                built_nodes.entry(parent).or_default().push(dir_node);
            }
        }
    }

    let mut root_nodes = Vec::new();

    if let Some(files) = files_by_parent.get("") {
        for entry in files.values() {
            root_nodes.push(TreemapNode::from_entry(entry));
        }
    }

    if let Some(root_dir_nodes) = built_nodes.remove("") {
        root_nodes.extend(root_dir_nodes);
    }

    root_nodes
}


