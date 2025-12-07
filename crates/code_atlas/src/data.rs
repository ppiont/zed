use gpui::{Bounds, Pixels};
use project::ProjectEntryId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use worktree::Entry;

/// Unique identifier for treemap nodes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeId {
    File(ProjectEntryId),
    Directory(ProjectEntryId),
}

/// A node in the hierarchical treemap
#[derive(Clone, Debug)]
pub struct TreemapNode {
    pub id: NodeId,
    #[allow(dead_code)]
    pub path: Arc<PathBuf>,
    #[allow(dead_code)]
    pub name: String,
    pub size: u64,
    #[allow(dead_code)]
    pub file_size_bytes: u64,
    pub children: Vec<TreemapNode>,
    #[allow(dead_code)]
    pub bounds: Bounds<Pixels>,
    #[allow(dead_code)]
    pub is_expanded: bool,
}

impl TreemapNode {
    pub fn from_entry(entry: &Entry, worktree_path: &Path) -> Self {
        let rel_path_str = entry.path.as_unix_str();
        let full_path = worktree_path.join(rel_path_str);
        let name = entry
            .path
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| rel_path_str.to_string());

        Self {
            id: if entry.is_file() {
                NodeId::File(entry.id)
            } else {
                NodeId::Directory(entry.id)
            },
            path: Arc::new(full_path),
            name,
            size: entry.size,
            file_size_bytes: entry.size,
            children: Vec::new(),
            bounds: Bounds::default(),
            is_expanded: false,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self.id, NodeId::File(_))
    }

    #[allow(dead_code)]
    pub fn is_directory(&self) -> bool {
        matches!(self.id, NodeId::Directory(_))
    }

    pub fn compute_aggregate_size(&mut self) {
        if !self.children.is_empty() {
            self.size = self.children.iter().map(|c| c.size).sum();
        }
    }
}

/// Builds hierarchical tree from flat file entries (iterative to avoid stack overflow)
pub fn build_tree(entries: impl Iterator<Item = Entry>, worktree_path: &Path) -> Vec<TreemapNode> {
    let mut files_by_parent: HashMap<String, Vec<Entry>> = HashMap::new();
    let mut directories: HashMap<String, Entry> = HashMap::new();
    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_dir_paths: Vec<String> = Vec::new();

    for entry in entries {
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
                .push(path_str.clone());
            all_dir_paths.push(path_str.clone());
            directories.insert(path_str, entry);
        } else {
            let parent = entry
                .path
                .parent()
                .map(|p| p.as_unix_str().to_string())
                .unwrap_or_default();
            files_by_parent.entry(parent).or_default().push(entry);
        }
    }

    // Sort directories by path depth (deepest first) so we process leaves before parents
    all_dir_paths.sort_by(|a, b| {
        let depth_a = a.matches('/').count();
        let depth_b = b.matches('/').count();
        depth_b.cmp(&depth_a)
    });

    // Build nodes bottom-up: process deepest directories first
    let mut built_nodes: HashMap<String, Vec<TreemapNode>> = HashMap::new();

    for dir_path in &all_dir_paths {
        let mut children = Vec::new();

        // Add file children
        if let Some(files) = files_by_parent.get(dir_path) {
            for entry in files {
                children.push(TreemapNode::from_entry(entry, worktree_path));
            }
        }

        // Add directory children (already built since we process deepest first)
        if let Some(child_dir_paths) = children_by_parent.get(dir_path) {
            for child_path in child_dir_paths {
                if let Some(mut child_nodes) = built_nodes.remove(child_path) {
                    children.append(&mut child_nodes);
                }
            }
        }

        // Create the directory node with its children
        if let Some(dir_entry) = directories.get(dir_path) {
            let mut dir_node = TreemapNode::from_entry(dir_entry, worktree_path);
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

    // Collect root nodes
    let mut root_nodes = Vec::new();

    // Add root-level files
    if let Some(files) = files_by_parent.get("") {
        for entry in files {
            root_nodes.push(TreemapNode::from_entry(entry, worktree_path));
        }
    }

    // Add root-level directories
    if let Some(root_dir_nodes) = built_nodes.remove("") {
        root_nodes.extend(root_dir_nodes);
    }

    root_nodes
}

/// Flattens a tree node for layout (shows children if expanded, otherwise just the node)
pub fn flatten_for_layout(node: &TreemapNode) -> Vec<&TreemapNode> {
    if node.is_expanded && !node.children.is_empty() {
        node.children.iter().flat_map(flatten_for_layout).collect()
    } else {
        vec![node]
    }
}
