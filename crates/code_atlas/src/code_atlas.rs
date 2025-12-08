mod animation;
mod colors;
mod data;
mod git_worker;
mod interaction;
mod loc_worker;
mod lod;
mod persistence;
mod treemap;

use animation::AnimationState;
use colors::activity_color;
use data::{build_tree, NodeId, TreemapNode};
use git_worker::{spawn_git_worker, GitResult};
use gpui::{
    actions, canvas, div, point, px, quad, App, Bounds, BorderStyle, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Point, Render, ScrollWheelEvent, SharedString, Size,
    Subscription, Task, TextRun, WeakEntity, Window,
};
use interaction::InteractionState;
use loc_worker::{spawn_loc_worker, LocResult};
use lod::{label_font_size, label_opacity, should_show_label};
use project::{Event as ProjectEvent, Project, ProjectEntryId};
use std::collections::HashMap;
use theme::ActiveTheme;
use ui::{prelude::*, Icon, IconName};
use worktree::PathChange;
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
    animation: AnimationState,
    #[allow(dead_code)]
    loc_loading: bool,
    #[allow(dead_code)]
    loc_task: Option<Task<()>>,
    #[allow(dead_code)]
    git_task: Option<Task<()>>,
    last_layout_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    hovered_node: Option<NodeId>,
    #[allow(dead_code)]
    _project_subscription: Subscription,
    pending_layout_update: Option<Task<()>>,
}

impl CodeAtlas {
    pub fn new(project: Entity<Project>, workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let root_nodes = Self::load_file_tree(&project, cx);

        let subscription = cx.subscribe(&project, Self::on_project_event);

        let mut atlas = Self {
            project,
            workspace,
            focus_handle: cx.focus_handle(),
            root_nodes,
            interaction: InteractionState::default(),
            loc_cache: HashMap::new(),
            git_cache: HashMap::new(),
            animation: AnimationState::default(),
            loc_loading: true,
            loc_task: None,
            git_task: None,
            last_layout_bounds: std::rc::Rc::new(std::cell::Cell::new(None)),
            hovered_node: None,
            _project_subscription: subscription,
            pending_layout_update: None,
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
                    // Capture initial layout BEFORE updating sizes (for animation)
                    if let Some(bounds) = this.last_layout_bounds.get() {
                        this.capture_initial_bounds_if_needed(bounds);
                    }

                    // Update loc cache and node sizes
                    for result in &results {
                        this.loc_cache.insert(result.entry_id, result.loc);
                    }
                    this.update_node_sizes();
                    this.loc_loading = false;

                    // Trigger animation to new layout
                    if let Some(bounds) = this.last_layout_bounds.get() {
                        this.update_animation_targets(bounds);
                        this.schedule_animation_frame(cx);
                    }
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
        for node in &mut self.root_nodes {
            if let NodeId::File(id) = node.id {
                if let Some(&loc) = self.loc_cache.get(&id) {
                    node.size = loc;
                }
            }
        }
    }

    fn capture_initial_bounds_if_needed(&mut self, bounds: Bounds<Pixels>) {
        if self.animation.has_initial_bounds() {
            return;
        }

        let layout_nodes = treemap::layout_tree(&self.root_nodes, bounds, self.interaction.zoom);
        let initial_bounds: HashMap<NodeId, Bounds<Pixels>> = layout_nodes
            .iter()
            .map(|node| (node.id, node.bounds))
            .collect();
        self.animation.set_initial_bounds(initial_bounds);
    }

    fn update_animation_targets(&mut self, bounds: Bounds<Pixels>) {
        let layout_nodes = treemap::layout_tree(&self.root_nodes, bounds, self.interaction.zoom);
        let new_targets: HashMap<NodeId, Bounds<Pixels>> = layout_nodes
            .iter()
            .map(|node| (node.id, node.bounds))
            .collect();
        self.animation.animate_to(new_targets);
    }

    fn schedule_animation_frame(&self, cx: &mut Context<Self>) {
        if self.animation.animating {
            cx.spawn(async move |this, cx| {
                smol::Timer::after(std::time::Duration::from_millis(16)).await;

                let _ = this.update(cx, |this, cx| {
                    if this.animation.update() {
                        this.schedule_animation_frame(cx);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn collect_all_animated_bounds(&self) -> HashMap<NodeId, Bounds<Pixels>> {
        self.root_nodes
            .iter()
            .filter_map(|node| {
                self.animation
                    .get_bounds(node.id)
                    .map(|bounds| (node.id, bounds))
            })
            .collect()
    }

    fn get_node_details(&self, node_id: NodeId) -> Option<(String, u64, Option<i64>)> {
        let node = self.root_nodes.iter().find(|n| n.id == node_id)?;
        let path = node.rel_path.to_string();
        let loc = node.size;
        let timestamp = match node_id {
            NodeId::File(entry_id) => self.git_cache.get(&entry_id).copied(),
            NodeId::Directory(_) => None,
        };
        Some((path, loc, timestamp))
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
            self.hovered_node = None;
            cx.notify();
        } else if let Some(bounds) = self.last_layout_bounds.get() {
            let new_hover = self.find_node_at_point(event.position, bounds);
            if new_hover != self.hovered_node {
                self.hovered_node = new_hover;
                cx.notify();
            }
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
                            self.handle_file_click(entry_id, event.click_count, window, cx);
                        }
                    }
                }
            }
            cx.notify();
        }
    }

    fn handle_file_click(
        &self,
        entry_id: ProjectEntryId,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = self.project.read(cx);

        let mut target = None;
        for worktree in project.visible_worktrees(cx) {
            let worktree_read = worktree.read(cx);
            if let Some(entry) = worktree_read.entry_for_id(entry_id) {
                target = Some((worktree_read.id(), entry.path.clone()));
                break;
            }
        }

        if let Some((worktree_id, path)) = target {
            let project_path = project::ProjectPath { worktree_id, path };
            // Single-click = preview tab, double-click = permanent tab
            let allow_preview = click_count == 1;

            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_path_preview(
                        project_path,
                        None,          // pane
                        true,          // focus_item
                        allow_preview, // allow_preview
                        true,          // activate
                        window,
                        cx,
                    )
                    .detach();
            });
        }
    }

    fn on_project_event(
        &mut self,
        _project: Entity<Project>,
        event: &ProjectEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ProjectEvent::WorktreeUpdatedEntries(worktree_id, changes) => {
                self.handle_file_changes(*worktree_id, changes, cx);
            }
            ProjectEvent::WorktreeAdded(_) | ProjectEvent::WorktreeRemoved(_) => {
                self.reload_file_tree(cx);
            }
            _ => {}
        }
    }

    fn handle_file_changes(
        &mut self,
        worktree_id: worktree::WorktreeId,
        changes: &worktree::UpdatedEntriesSet,
        cx: &mut Context<Self>,
    ) {
        let mut needs_layout = false;
        let mut files_to_recount = Vec::new();

        for (_path, entry_id, change) in changes.iter() {
            match change {
                PathChange::Added | PathChange::AddedOrUpdated => {
                    needs_layout = true;
                }
                PathChange::Removed => {
                    self.loc_cache.remove(entry_id);
                    self.git_cache.remove(entry_id);
                    needs_layout = true;
                }
                PathChange::Updated => {
                    self.loc_cache.remove(entry_id);
                    needs_layout = true;

                    if let Some(full_path) = self.get_entry_path(*entry_id, worktree_id, cx) {
                        files_to_recount.push((*entry_id, full_path));
                    }
                }
                PathChange::Loaded => {}
            }
        }

        if !files_to_recount.is_empty() {
            self.queue_loc_updates(files_to_recount, cx);
        }

        if needs_layout {
            self.schedule_layout_update(cx);
        }
    }

    fn get_entry_path(
        &self,
        entry_id: ProjectEntryId,
        worktree_id: worktree::WorktreeId,
        cx: &Context<Self>,
    ) -> Option<std::path::PathBuf> {
        let project = self.project.read(cx);
        for worktree in project.visible_worktrees(cx) {
            let wt = worktree.read(cx);
            if wt.id() == worktree_id {
                if let Some(entry) = wt.entry_for_id(entry_id) {
                    return Some(wt.abs_path().join(entry.path.as_unix_str()));
                }
            }
        }
        None
    }

    fn schedule_layout_update(&mut self, cx: &mut Context<Self>) {
        self.pending_layout_update.take();

        self.pending_layout_update = Some(cx.spawn(async move |this, cx| {
            smol::Timer::after(std::time::Duration::from_millis(100)).await;

            let _ = this.update(cx, |this, cx| {
                if let Some(bounds) = this.last_layout_bounds.get() {
                    this.capture_initial_bounds_if_needed(bounds);
                }

                this.root_nodes = Self::load_file_tree(&this.project, cx);
                this.update_node_sizes();

                if let Some(bounds) = this.last_layout_bounds.get() {
                    this.update_animation_targets(bounds);
                    this.schedule_animation_frame(cx);
                }

                cx.notify();
            });
        }));
    }

    fn reload_file_tree(&mut self, cx: &mut Context<Self>) {
        self.root_nodes = Self::load_file_tree(&self.project, cx);
        self.update_node_sizes();

        self.start_loc_loading(cx);
        self.start_git_loading(cx);

        cx.notify();
    }

    fn queue_loc_updates(
        &mut self,
        files: Vec<(ProjectEntryId, std::path::PathBuf)>,
        cx: &mut Context<Self>,
    ) {
        let weak_self = cx.weak_entity();

        cx.spawn(async move |_this, cx| {
            for (entry_id, path) in files {
                if let Ok(loc) = loc_worker::count_lines(&path) {
                    let _ = weak_self.update(cx, |this, cx| {
                        this.loc_cache.insert(entry_id, loc);
                        this.update_node_sizes();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn find_node_at_point(&self, target_point: Point<Pixels>, bounds: Bounds<Pixels>) -> Option<NodeId> {
        let layout_nodes = treemap::layout_tree(&self.root_nodes, bounds, self.interaction.zoom);
        let zoom = self.interaction.zoom;
        let pan = self.interaction.pan_offset;

        for node in &layout_nodes {
            let origin_x = node.bounds.origin.x * zoom + pan.x;
            let origin_y = node.bounds.origin.y * zoom + pan.y;
            let width = node.bounds.size.width * zoom;
            let height = node.bounds.size.height * zoom;

            let screen_bounds = Bounds::new(
                gpui::point(origin_x, origin_y),
                Size { width, height },
            );

            if screen_bounds.contains(&target_point) {
                return Some(node.id);
            }
        }
        None
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
        let zoom = self.interaction.zoom;
        let pan_offset = self.interaction.pan_offset;

        // Clone root nodes for layout closure
        let root_nodes = self.root_nodes.clone();

        // Clone git cache for paint closure
        let git_cache = self.git_cache.clone();

        let last_layout_bounds = self.last_layout_bounds.clone();

        // Capture current animated bounds for this frame
        let animating = self.animation.animating;
        let animated_bounds: HashMap<NodeId, Bounds<Pixels>> = if animating {
            // Collect current animated bounds for all nodes
            self.collect_all_animated_bounds()
        } else {
            HashMap::new()
        };

        // Get hovered node details for tooltip
        let hovered_details = self.hovered_node.and_then(|id| self.get_node_details(id));

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

                        for node in &layout_nodes {
                            let world_bounds = if animating {
                                animated_bounds.get(&node.id).copied().unwrap_or(node.bounds)
                            } else {
                                node.bounds
                            };

                            let origin_x = world_bounds.origin.x * zoom + pan_offset.x;
                            let origin_y = world_bounds.origin.y * zoom + pan_offset.y;
                            let width = world_bounds.size.width * zoom;
                            let height = world_bounds.size.height * zoom;

                            if origin_x > vp.size.width
                                || origin_y > vp.size.height
                                || origin_x + width < px(0.)
                                || origin_y + height < px(0.)
                            {
                                continue;
                            }

                            let screen_bounds =
                                Bounds::new(point(origin_x, origin_y), Size { width, height });

                            let bg = if let NodeId::File(id) = node.id {
                                let timestamp = git_cache.get(&id).copied();
                                activity_color(timestamp, cx)
                            } else {
                                gpui::white()
                            };

                            window.paint_quad(quad(
                                screen_bounds,
                                px(2.),
                                bg,
                                gpui::Edges::all(px(1.)),
                                border_color,
                                BorderStyle::Solid,
                            ));

                            if should_show_label(screen_bounds) {
                                paint_label(&node.name, screen_bounds, window, cx);
                            }
                        }
                    },
                )
                .size_full(),
            )
            .when_some(hovered_details, |container, (path, loc, timestamp)| {
                let days_ago = timestamp.map(|ts| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    (now - ts) / 86400
                });

                container.child(
                    div()
                        .absolute()
                        .top_2()
                        .left_2()
                        .bg(cx.theme().colors().elevated_surface_background)
                        .border_1()
                        .border_color(cx.theme().colors().border)
                        .rounded_md()
                        .shadow_md()
                        .p_2()
                        .max_w(px(400.))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().colors().text)
                                .child(path),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().colors().text_muted)
                                .child(format!("{} lines", loc)),
                        )
                        .when_some(days_ago, |tooltip, days| {
                            tooltip.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(if days == 0 {
                                        "Modified today".to_string()
                                    } else if days == 1 {
                                        "Modified yesterday".to_string()
                                    } else {
                                        format!("Modified {} days ago", days)
                                    }),
                            )
                        }),
                )
            })
    }
}

