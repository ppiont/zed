mod data;
mod interaction;
mod treemap;

use data::{build_tree, TreemapNode};
use gpui::{
    actions, canvas, div, point, px, quad, App, Bounds, BorderStyle, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Render, ScrollWheelEvent, SharedString, Size, Window,
};
use interaction::InteractionState;
use project::Project;
use treemap::squarify;
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
    #[allow(dead_code)]
    project: Entity<Project>,
    focus_handle: FocusHandle,
    root_nodes: Vec<TreemapNode>,
    interaction: InteractionState,
}

impl CodeAtlas {
    pub fn new(project: Entity<Project>, cx: &mut Context<Self>) -> Self {
        let root_nodes = Self::load_file_tree(&project, cx);

        Self {
            project,
            focus_handle: cx.focus_handle(),
            root_nodes,
            interaction: InteractionState::default(),
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left || event.button == MouseButton::Middle {
            self.interaction.stop_pan();
            cx.notify();
        }
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
        let view = cx.new(|cx| CodeAtlas::new(project, cx));
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
        let file_color = cx.theme().colors().element_selected;

        // Collect all files from the tree (flatten completely)
        let files: Vec<_> = self
            .root_nodes
            .iter()
            .flat_map(|n| collect_all_files(n))
            .collect();

        let sizes: Vec<(usize, f64)> = files
            .iter()
            .enumerate()
            .map(|(i, n)| (i, n.size as f64))
            .filter(|(_, s)| *s > 0.0)
            .collect();

        let zoom = self.interaction.zoom;
        let pan_offset = self.interaction.pan_offset;

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
                    move |bounds, _, _| squarify(&sizes, bounds),
                    move |bounds, layout_nodes, window, _cx| {
                        let vp_x: f32 = bounds.origin.x.into();
                        let vp_y: f32 = bounds.origin.y.into();
                        let vp_w: f32 = bounds.size.width.into();
                        let vp_h: f32 = bounds.size.height.into();
                        let pan_x: f32 = pan_offset.x.into();
                        let pan_y: f32 = pan_offset.y.into();

                        for layout_node in layout_nodes {
                            // Apply pan and zoom transformation
                            let node_x: f32 = layout_node.bounds.origin.x.into();
                            let node_y: f32 = layout_node.bounds.origin.y.into();
                            let node_w: f32 = layout_node.bounds.size.width.into();
                            let node_h: f32 = layout_node.bounds.size.height.into();
                            let origin_x = node_x * zoom + pan_x;
                            let origin_y = node_y * zoom + pan_y;
                            let width = node_w * zoom;
                            let height = node_h * zoom;

                            // Cull off-screen rectangles for performance
                            if origin_x + width < vp_x
                                || origin_y + height < vp_y
                                || origin_x > vp_x + vp_w
                                || origin_y > vp_y + vp_h
                            {
                                continue;
                            }

                            let screen_bounds = Bounds::new(
                                point(px(origin_x), px(origin_y)),
                                Size {
                                    width: px(width),
                                    height: px(height),
                                },
                            );

                            window.paint_quad(quad(
                                screen_bounds,
                                px(2.),
                                file_color,
                                gpui::Edges::all(px(1.)),
                                border_color,
                                BorderStyle::Solid,
                            ));
                        }
                    },
                )
                .size_full(),
            )
    }
}

fn collect_all_files(node: &TreemapNode) -> Vec<&TreemapNode> {
    if node.is_file() {
        vec![node]
    } else {
        node.children.iter().flat_map(collect_all_files).collect()
    }
}
