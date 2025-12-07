mod treemap;

use gpui::{
    actions, canvas, div, fill, App, Context, EventEmitter, FocusHandle, Focusable, IntoElement,
    Render, SharedString, Window,
};
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
    focus_handle: FocusHandle,
}

impl CodeAtlas {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn open(
        workspace: &mut Workspace,
        _action: &Open,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let view = cx.new(|cx| CodeAtlas::new(cx));
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
        let colors = [
            cx.theme().status().modified,
            cx.theme().status().created,
            cx.theme().status().info,
            cx.theme().status().warning,
        ];

        let test_sizes: Vec<(usize, f64)> = vec![
            (0, 1000.0),
            (1, 800.0),
            (2, 600.0),
            (3, 400.0),
            (4, 300.0),
            (5, 200.0),
            (6, 150.0),
            (7, 100.0),
        ];

        div()
            .size_full()
            .bg(bg)
            .child(
                canvas(
                    move |bounds, _, _| squarify(&test_sizes, bounds),
                    move |_bounds, nodes, window, _| {
                        for node in nodes {
                            let color = colors[node.id % colors.len()];
                            window.paint_quad(fill(node.bounds, color));
                        }
                    },
                )
                .size_full(),
            )
    }
}
