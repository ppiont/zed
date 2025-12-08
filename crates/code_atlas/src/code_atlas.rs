mod colors;
mod data;
mod git_worker;
mod interaction;
mod loc_worker;
mod lod;
mod persistence;
mod treemap;

use colors::activity_color;
use data::{build_tree, NodeId, TreemapNode};
use git_worker::{spawn_git_worker, GitResult};
use gpui::{
    actions, canvas, div, point, px, quad, App, Bounds, BorderStyle, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Point, Render, ScrollWheelEvent, SharedString, Size, Task,
    TextRun, WeakEntity, Window,
};
use lod::{label_font_size, label_opacity, should_show_label};
use interaction::InteractionState;
use loc_worker::{spawn_loc_worker, LocResult};
use project::{Project, ProjectEntryId};
use std::collections::HashMap;
use theme::ActiveTheme;
use ui::{prelude::*, Icon, IconName};
use workspace::item::ItemEvent;
use workspace::{Item, Workspace};

actions!(code_atlas, [Open]);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(CodeAtlas::open);
    })
    .detach();
}

pub struct CodeAtlas {
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    root_nodes: Vec<TreemapNode>,
    interaction: InteractionState,
    loc_cache: HashMap<ProjectEntryId, u64>,
    git_cache: HashMap<ProjectEntryId, i64>,
    #[allow(dead_code)]
    loc_loading: bool,
    #[allow(dead_code)]
    loc_task: Option<Task<()>>,
    #[allow(dead_code)]
    git_task: Option<Task<()>>,
    last_layout_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl CodeAtlas {
    pub fn new(project: Entity<Project>, workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let root_nodes = Self::load_file_tree(&project, cx);

        let mut atlas = Self {
            project,
            workspace,
            focus_handle: cx.focus_handle(),
            root_nodes,
            interaction: InteractionState::default(),
            loc_cache: HashMap::new(),
            git_cache: HashMap::new(),
            loc_loading: true,
            loc_task: None,
            git_task: None,
            last_layout_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
        };

        atlas.start_loc_loading(cx);
        atlas.start_git_loading(cx);
        atlas
    }

    fn start_loc_loading(&mut self, cx: &mut Context<Self>) {
        let project = self.project.read(cx);
        let mut files_to_count = Vec::new();

        for worktree in project.visible_worktrees(cx) {
            let worktree = worktree.read(cx);
            let worktree_id = worktree.id().to_proto() as i64;
            let snapshot = worktree.snapshot();
            let worktree_path = snapshot.abs_path();

            for entry in snapshot.files(false, 0) {
                let full_path = worktree_path.join(entry.path.as_unix_str());
                let (mtime_s, mtime_ns) = entry
                    .mtime
                    .as_ref()
                    .and_then(|m| m.to_seconds_and_nanos_for_persistence())
                    .map(|(s, ns)| (s as i64, ns as i32))
                    .unwrap_or((0, 0));

                files_to_count.push((
                    entry.id,
                    full_path.to_path_buf(),
                    worktree_id,
                    mtime_s,
                    mtime_ns,
                ));
            }
        }

        let weak_self = cx.weak_entity();
        let executor = cx.background_executor().clone();

        self.loc_task = Some(cx.spawn(async move |_this, cx| {
            let (tx, rx) = smol::channel::bounded::<Vec<LocResult>>(1);

            let _worker = spawn_loc_worker(executor, files_to_count, move |results| {
                let _ = tx.send_blocking(results);
            });

            while let Ok(results) = rx.recv().await {
                let _ = weak_self.update(cx, |this, cx| {
                    for result in &results {
                        this.loc_cache.insert(result.entry_id, result.loc);
                    }
                    this.update_node_sizes();
                    this.loc_loading = false;
                    cx.notify();
                });
            }
        }));
    }

    fn start_git_loading(&mut self, cx: &mut Context<Self>) {
        let project = self.project.read(cx);
        let mut files_to_check = Vec::new();

        // Assuming single repo for now or flattening
        // We need repository path.
        // worktree.root_entry() usually? or worktree.abs_path().
        
        // We'll spawn one worker per worktree to handle different repos correctly
        // But for simplicity of this impl (Phase 6), let's just grab all files and assume one git root per worktree.
        
        let mut worktree_roots = Vec::new();

        for worktree in project.visible_worktrees(cx) {
            let worktree = worktree.read(cx);
            let worktree_id = worktree.id().to_proto() as i64;
            let snapshot = worktree.snapshot();
            let worktree_path = snapshot.abs_path().to_path_buf();
            
            // Only count if it's a git repo?
            // checking .git exists is simple check.
            let git_dir = worktree_path.join(".git");
            if !git_dir.exists() {
                 continue;
            }
            
            worktree_roots.push((worktree_path.clone(), worktree_id));

             for entry in snapshot.files(false, 0) {
                  let full_path = worktree_path.join(entry.path.as_unix_str());
                  
                  let (mtime_s, mtime_ns) = entry
                      .mtime
                      .as_ref()
                      .and_then(|m| m.to_seconds_and_nanos_for_persistence())
                      .map(|(s, ns)| (s as i64, ns as i32))
                      .unwrap_or((0, 0));

                  files_to_check.push((
                     entry.id,
                     full_path.to_path_buf(),
                     worktree_id,
                     mtime_s,
                     mtime_ns
                  ));
             }
        }

        if files_to_check.is_empty() {
            return;
        }
        
        // Just take the first worktree root for now (Limitation: specific to single repo open usually)
        // If multi-root, we should group files by root.
        // For Proof of Concept, let's use the first one.
        let repo_path = if let Some((path, _)) = worktree_roots.first() {
            path.clone()
        } else {
             return;
        };

        let weak_self = cx.weak_entity();
        let executor = cx.background_executor().clone();

        // Correcting the above: 
        // 1. Channel needs to be Unbounded or we need a drainage loop concurrent with worker.
        // 2. spawn_git_worker executes loop in background spawn.
        
        self.git_task = Some(cx.spawn(async move |_this, cx| {
            let (tx, rx) = smol::channel::unbounded::<GitResult>();

            let _worker = spawn_git_worker(executor, repo_path, files_to_check, move |result| {
                let _ = tx.send_blocking(result);
            });

            // Limit updates to UI to avoid flooding
            let mut batch = Vec::new();
            let mut last_update = std::time::Instant::now();

            while let Ok(result) = rx.recv().await {
                batch.push(result);

                if batch.len() > 100 || last_update.elapsed().as_millis() > 100 {
                    let current_batch = std::mem::take(&mut batch);
                    let _ = weak_self.update(cx, |this, cx| {
                        for res in current_batch {
                            if let Some(ts) = res.timestamp {
                                this.git_cache.insert(res.entry_id, ts);
                            }
                        }
                        cx.notify();
                    });
                    last_update = std::time::Instant::now();
                }
            }
            // Flush remaining
            if !batch.is_empty() {
                let _ = weak_self.update(cx, |this, cx| {
                    for res in batch {
                        if let Some(ts) = res.timestamp {
                            this.git_cache.insert(res.entry_id, ts);
                        }
                    }
                    cx.notify();
                });
            }
        }));
    }

    fn update_node_sizes(&mut self) {
        fn update_recursive(node: &mut TreemapNode, cache: &HashMap<ProjectEntryId, u64>) {
            if let NodeId::File(id) = node.id {
                if let Some(&loc) = cache.get(&id) {
                    node.size = loc;
                }
            }
            for child in &mut node.children {
                update_recursive(child, cache);
            }
            node.compute_aggregate_size();
        }

        for node in &mut self.root_nodes {
            update_recursive(node, &self.loc_cache);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left || event.button == MouseButton::Middle {
            self.interaction.start_pan(event.position);
            cx.notify();
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.interaction.is_panning {
            self.interaction.update_pan(event.position);
            cx.notify();
        }
    }

    fn on_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left || event.button == MouseButton::Middle {
            let was_click = self.interaction.stop_pan();

            if was_click && event.button == MouseButton::Left {
                if let Some(bounds) = self.last_layout_bounds.get() {
                    if let Some(node_id) = self.find_node_at_point(event.position, bounds) {
                        if let NodeId::File(entry_id) = node_id {
                            if let Some(workspace) = self.workspace.upgrade() {
                                let project = self.project.read(cx);
                                let worktrees: Vec<_> = project.visible_worktrees(cx).collect();
                                let mut target = None;

                                for worktree in worktrees {
                                    let worktree_read = worktree.read(cx);
                                    if let Some(entry) = worktree_read.entry_for_id(entry_id) {
                                        target = Some((worktree_read.id(), entry.path.clone()));
                                        break;
                                    }
                                }

                                if let Some((worktree_id, path)) = target {
                                    let project_path = project::ProjectPath {
                                        worktree_id,
                                        path,
                                    };

                                    workspace.update(cx, |workspace, cx| {
                                        workspace.open_path(
                                            project_path,
                                            None,
                                            true,
                                            window,
                                            cx
                                        ).detach();
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cx.notify();
        }
    }

    fn find_node_at_point(&self, target_point: Point<Pixels>, bounds: Bounds<Pixels>) -> Option<NodeId> {
        // Re-run layout logic to find the node with the same LOD as rendered
        let layout_nodes = treemap::layout_tree(&self.root_nodes, bounds, self.interaction.zoom);
        
        fn find_recursive(
            nodes: &[treemap::RecursiveLayoutNode], 
            target_point: Point<Pixels>,
            zoom: f32,
            pan: Point<Pixels>
        ) -> Option<NodeId> {
            for node in nodes {
                 let origin_x = node.bounds.origin.x * zoom + pan.x;
                 let origin_y = node.bounds.origin.y * zoom + pan.y;
                 let width = node.bounds.size.width * zoom;
                 let height = node.bounds.size.height * zoom;
                 
                 let screen_bounds = Bounds::new(
                     gpui::point(origin_x, origin_y),
                     Size { width, height }
                 );
                 
                 if screen_bounds.contains(&target_point) {
                     // Check children first (top-down, but visually children are inside)
                     if let Some(id) = find_recursive(&node.children, target_point, zoom, pan) {
                         return Some(id);
                     }
                     // If no children matched (or leaf), return this node
                     return Some(node.id);
                 }
            }
            None
        }

        find_recursive(
            &layout_nodes, 
            target_point, 
            self.interaction.zoom, 
            self.interaction.pan_offset
        )
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let delta = match event.delta {
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / 100.0,
            gpui::ScrollDelta::Lines(l) => l.y / 3.0,
        };
        self.interaction.zoom_at(event.position, delta);
        cx.notify();
    }

    fn load_file_tree(project: &Entity<Project>, cx: &Context<Self>) -> Vec<TreemapNode> {
        let project = project.read(cx);
        let mut all_nodes = Vec::new();

        for worktree in project.visible_worktrees(cx) {
            let worktree = worktree.read(cx);
            let snapshot = worktree.snapshot();
            let worktree_path = snapshot.abs_path();

            let entries = snapshot.entries(false, 0);
            let mut nodes = build_tree(entries.cloned(), worktree_path.as_ref());
            all_nodes.append(&mut nodes);
        }

        all_nodes
    }

    pub fn open(
        workspace: &mut Workspace,
        _action: &Open,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let project = workspace.project().clone();
        let workspace_handle = workspace.weak_handle();
        let view = cx.new(|cx| CodeAtlas::new(project, workspace_handle, cx));
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(Box::new(view), true, true, None, window, cx);
        });
    }
}

impl EventEmitter<ItemEvent> for CodeAtlas {}

impl Focusable for CodeAtlas {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for CodeAtlas {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Code Atlas".into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::ListTree))
    }

    fn to_item_events(event: &Self::Event, mut f: impl FnMut(ItemEvent)) {
        f(*event);
    }
}


impl Render for CodeAtlas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().colors().surface_background;
        let border_color = cx.theme().colors().border;
        let dir_color = cx.theme().colors().surface_background; // Transparent/bg for directories
        let zoom = self.interaction.zoom;
        let pan_offset = self.interaction.pan_offset;
        
        // Clone root nodes for layout closure
        // Note: cloning the tree structure is relatively cheap (Arc path, strings, etc) compared to layout
        let root_nodes = self.root_nodes.clone();
        
        // Clone git cache for paint closure (expensive? 50k items map)
        // Optimization: Use ID-based lookup or shared immutable reference via Arc/Rc?
        // Cloning HashMap of i64 is okay-ish but not ideal every frame.
        // For now, let's clone.
        let git_cache = self.git_cache.clone();

        let last_layout_bounds = self.last_layout_bounds.clone();

        div()
            .size_full()
            .bg(bg)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(
                canvas(
                    move |bounds, _, _| {
                        last_layout_bounds.set(Some(bounds));
                        treemap::layout_tree(&root_nodes, bounds, zoom)
                    },
                    move |bounds, layout_nodes, window, cx| {
                        let vp = Bounds::new(point(px(0.), px(0.)), bounds.size);

                        fn paint_label(
                            name: &str,
                            screen_bounds: Bounds<Pixels>,
                            window: &mut Window,
                            cx: &mut App,
                        ) {
                            let font_size_f32 = label_font_size(screen_bounds);
                            let opacity = label_opacity(screen_bounds);
                            let font_size = px(font_size_f32);

                            // Get text color with opacity
                            let mut text_color = cx.theme().colors().text;
                            text_color.a *= opacity;

                            // Truncate name to fit (estimate ~0.6 em per character)
                            let screen_width: f32 = screen_bounds.size.width.into();
                            let max_chars = (screen_width / (font_size_f32 * 0.6)) as usize;
                            let char_count = name.chars().count();
                            let display_name: String = if char_count > max_chars && max_chars > 1 {
                                format!(
                                    "{}…",
                                    name.chars()
                                        .take(max_chars.saturating_sub(1))
                                        .collect::<String>()
                                )
                            } else {
                                name.to_string()
                            };

                            if display_name.is_empty() {
                                return;
                            }

                            // Create text run for the label using default font
                            let text_run = TextRun {
                                len: display_name.len(),
                                color: text_color,
                                ..Default::default()
                            };

                            // Shape the text line
                            let shaped_line = window.text_system().shape_line(
                                display_name.into(),
                                font_size,
                                &[text_run],
                                None, // No fixed cell width
                            );

                            // Calculate text position (left-aligned with padding, vertically centered)
                            let line_height = font_size * 1.2;
                            let text_origin = point(
                                screen_bounds.origin.x + px(4.),
                                screen_bounds.origin.y
                                    + (screen_bounds.size.height - line_height) / 2.,
                            );

                            // Paint the shaped line
                            let _ = shaped_line.paint(text_origin, line_height, window, cx);
                        }

                        fn paint_recursive(
                            nodes: Vec<treemap::RecursiveLayoutNode>,
                            window: &mut Window,
                            cx: &mut App,
                            vp: Bounds<Pixels>,
                            zoom: f32,
                            pan: Point<Pixels>,
                            dir_color: gpui::Hsla,
                            border_color: gpui::Hsla,
                            git_cache: &HashMap<ProjectEntryId, i64>,
                        ) {
                            for node in nodes {
                                // Apply transforms
                                let origin_x = node.bounds.origin.x * zoom + pan.x;
                                let origin_y = node.bounds.origin.y * zoom + pan.y;
                                let width = node.bounds.size.width * zoom;
                                let height = node.bounds.size.height * zoom;

                                // Cull
                                if origin_x > vp.size.width
                                    || origin_y > vp.size.height
                                    || origin_x + width < px(0.)
                                    || origin_y + height < px(0.)
                                {
                                    continue;
                                }

                                let screen_bounds =
                                    Bounds::new(point(origin_x, origin_y), Size { width, height });

                                // Determine node type
                                let is_file = matches!(node.id, NodeId::File(_));
                                let is_collapsed_dir =
                                    matches!(node.id, NodeId::Directory(_)) && node.children.is_empty();

                                // Determine background color
                                let bg = if is_file {
                                    if let NodeId::File(id) = node.id {
                                        let timestamp = git_cache.get(&id).copied();
                                        activity_color(timestamp, cx)
                                    } else {
                                        gpui::white()
                                    }
                                } else if is_collapsed_dir {
                                    // Collapsed directory - use element_background for distinction
                                    cx.theme().colors().element_background
                                } else {
                                    dir_color
                                };

                                // Draw rectangle with corner radius for visual polish
                                window.paint_quad(quad(
                                    screen_bounds,
                                    px(2.), // Corner radius
                                    bg,
                                    gpui::Edges::all(px(1.)),
                                    border_color,
                                    BorderStyle::Solid,
                                ));

                                // Draw label if large enough (only for files and collapsed directories)
                                if (is_file || is_collapsed_dir) && should_show_label(screen_bounds)
                                {
                                    paint_label(&node.name, screen_bounds, window, cx);
                                }

                                // Recurse into children
                                if !node.children.is_empty() {
                                    paint_recursive(
                                        node.children,
                                        window,
                                        cx,
                                        vp,
                                        zoom,
                                        pan,
                                        dir_color,
                                        border_color,
                                        git_cache,
                                    );
                                }
                            }
                        }

                        paint_recursive(
                            layout_nodes,
                            window,
                            cx,
                            vp,
                            zoom,
                            pan_offset,
                            dir_color,
                            border_color,
                            &git_cache,
                        );
                    },
                )
                .size_full(),
            )
    }
}

