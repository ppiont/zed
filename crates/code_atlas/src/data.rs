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

/// Builds hierarchical tree from flat file entries
pub fn build_tree(entries: impl Iterator<Item = Entry>, worktree_path: &Path) -> Vec<TreemapNode> {
    let mut files_by_parent: HashMap<String, Vec<Entry>> = HashMap::new();
    let mut directories: HashMap<String, Entry> = HashMap::new();

    for entry in entries {
        let path_str = entry.path.as_unix_str().to_string();
        if entry.is_dir() {
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

    fn build_node(
        path: &str,
        files_by_parent: &HashMap<String, Vec<Entry>>,
        directories: &HashMap<String, Entry>,
        worktree_path: &Path,
    ) -> Vec<TreemapNode> {
        let mut nodes = Vec::new();

        if let Some(files) = files_by_parent.get(path) {
            for entry in files {
                nodes.push(TreemapNode::from_entry(entry, worktree_path));
            }
        }

        for (dir_path_str, dir_entry) in directories.iter() {
            let dir_path = dir_entry.path.as_ref();
            let parent_str = dir_path
                .parent()
                .map(|p| p.as_unix_str())
                .unwrap_or("");

            let matches = parent_str == path;

            if matches {
                let mut dir_node = TreemapNode::from_entry(dir_entry, worktree_path);
                dir_node.children =
                    build_node(dir_path_str, files_by_parent, directories, worktree_path);
                dir_node.compute_aggregate_size();
                if dir_node.size > 0 {
                    nodes.push(dir_node);
                }
            }
        }

        nodes
    }

    build_node("", &files_by_parent, &directories, worktree_path)
}

/// Flattens a tree node for layout (shows children if expanded, otherwise just the node)
pub fn flatten_for_layout(node: &TreemapNode) -> Vec<&TreemapNode> {
    if node.is_expanded && !node.children.is_empty() {
        node.children.iter().flat_map(flatten_for_layout).collect()
    } else {
        vec![node]
    }
}
