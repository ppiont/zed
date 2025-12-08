use gpui::{px, Bounds, Pixels, Point, Size};
use streemap::Rect;

use crate::data::{NodeId, TreemapNode};
use crate::lod::should_expand;

#[derive(Debug)]
pub struct RecursiveLayoutNode {
    pub id: NodeId,
    pub name: String,
    pub bounds: Bounds<Pixels>,
    pub children: Vec<RecursiveLayoutNode>,
}

/// Recursively computes layout for a list of nodes using the streemap crate
/// The zoom parameter is used to calculate screen-space bounds for LOD decisions
pub fn layout_tree(
    nodes: &[TreemapNode],
    bounds: Bounds<Pixels>,
    zoom: f32,
) -> Vec<RecursiveLayoutNode> {
    if nodes.is_empty() {
        return Vec::new();
    }

    // Filter nodes with positive size and create items for streemap
    let mut items: Vec<(usize, f64, Rect<f64>)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.display_size() > 0)
        .map(|(i, n)| {
            (
                i,
                n.display_size() as f64,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                },
            )
        })
        .collect();

    if items.is_empty() {
        return Vec::new();
    }

    // Create bounds rect for streemap
    let streemap_bounds = Rect {
        x: f64::from(bounds.origin.x),
        y: f64::from(bounds.origin.y),
        w: f64::from(bounds.size.width),
        h: f64::from(bounds.size.height),
    };

    // Run the squarified treemap algorithm
    streemap::squarify(
        streemap_bounds,
        &mut items[..],
        |&(_, size, _)| size,
        |(_, _, item_rect), r| *item_rect = r,
    );

    // Convert results back to our format
    let mut recursive_layout = Vec::with_capacity(items.len());

    for (idx, _, item_rect) in &items {
        let node = &nodes[*idx];

        let gpui_bounds = Bounds::new(
            Point::new(px(item_rect.x as f32), px(item_rect.y as f32)),
            Size {
                width: px(item_rect.w as f32),
                height: px(item_rect.h as f32),
            },
        );

        // Calculate screen bounds for LOD decision (world bounds * zoom)
        let width: f32 = gpui_bounds.size.width.into();
        let height: f32 = gpui_bounds.size.height.into();
        let screen_bounds = Bounds::new(
            gpui_bounds.origin,
            Size {
                width: px(width * zoom),
                height: px(height * zoom),
            },
        );

        // Decide whether to expand children based on screen size
        let children =
            if node.is_directory() && !node.children.is_empty() && should_expand(screen_bounds) {
                // Apply padding for directory nesting
                let padding = px(4.);
                let inner_bounds = Bounds::new(
                    Point::new(
                        gpui_bounds.origin.x + padding,
                        gpui_bounds.origin.y + padding,
                    ),
                    Size {
                        width: (gpui_bounds.size.width - padding * 2.).max(px(0.)),
                        height: (gpui_bounds.size.height - padding * 2.).max(px(0.)),
                    },
                );
                layout_tree(&node.children, inner_bounds, zoom)
            } else {
                Vec::new()
            };

        recursive_layout.push(RecursiveLayoutNode {
            id: node.id,
            name: node.name.clone(),
            bounds: gpui_bounds,
            children,
        });
    }

    recursive_layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_aspect_ratios() {
        // Test the streemap crate directly
        let mut items: Vec<(usize, f64, Rect<f64>)> = vec![
            (0, 6.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
            (1, 6.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
            (2, 4.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
            (3, 3.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
            (4, 2.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
            (5, 1.0, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }),
        ];

        let bounds = Rect { x: 0.0, y: 0.0, w: 600.0, h: 400.0 };

        streemap::squarify(
            bounds,
            &mut items[..],
            |&(_, size, _)| size,
            |(_, _, item_rect), r| *item_rect = r,
        );

        for (i, _, rect) in &items {
            let ratio = if rect.w > rect.h {
                rect.w / rect.h
            } else {
                rect.h / rect.w
            };
            assert!(
                ratio < 5.0,
                "Item {} has bad aspect ratio {}: {}x{}",
                i,
                ratio,
                rect.w,
                rect.h
            );
        }
    }
}
