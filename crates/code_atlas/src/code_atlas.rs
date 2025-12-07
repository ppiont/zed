mod data;
mod treemap;

use data::{build_tree, flatten_for_layout, TreemapNode};
use gpui::{
    actions, canvas, div, px, quad, App, BorderStyle, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, Render, SharedString, Window,
};
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
}

impl CodeAtlas {
    pub fn new(project: Entity<Project>, cx: &mut Context<Self>) -> Self {
        let root_nodes = Self::load_file_tree(&project, cx);

        Self {
            project,
            focus_handle: cx.focus_handle(),
            root_nodes,
        }
    }

    fn load_file_tree(project: &Entity<Project>, cx: &Context<Self>) -> Vec<TreemapNode> {
        let project = project.read(cx);
        let mut all_nodes = Vec::new();

        for worktree in project.worktree_store().read(cx).worktrees() {
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
        let file_color = cx.theme().colors().element_background;
        let dir_color = cx.theme().colors().surface_background;

        let nodes: Vec<_> = self
            .root_nodes
            .iter()
            .flat_map(|n| flatten_for_layout(n))
            .collect();

        let sizes: Vec<(usize, f64)> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (i, n.size as f64))
            .filter(|(_, s)| *s > 0.0)
            .collect();

        let node_colors: Vec<_> = nodes
            .iter()
            .map(|n| if n.is_file() { file_color } else { dir_color })
            .collect();

        div()
            .size_full()
            .bg(bg)
            .child(
                canvas(
                    move |bounds, _, _| squarify(&sizes, bounds),
                    move |_bounds, layout_nodes, window, _cx| {
                        for layout_node in layout_nodes {
                            let color = node_colors
                                .get(layout_node.id)
                                .copied()
                                .unwrap_or(border_color);
                            window.paint_quad(quad(
                                layout_node.bounds,
                                px(2.),
                                color,
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
