use gpui::{px, Bounds, Pixels, Point, Size};
use streemap::Rect;

use crate::data::{NodeId, TreemapNode};

#[derive(Debug)]
pub struct LayoutNode {
    pub id: NodeId,
    pub name: String,
    pub bounds: Bounds<Pixels>,
    pub children: Vec<LayoutNode>,
}

/// Recursively computes layout for nodes using the squarified treemap algorithm
pub fn layout_tree(
    nodes: &[TreemapNode],
    bounds: Bounds<Pixels>,
    _zoom: f32,
) -> Vec<LayoutNode> {
    if nodes.is_empty() {
        return Vec::new();
    }

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

    // Sort by size descending - squarify works best with sorted input
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let streemap_bounds = Rect {
        x: f64::from(bounds.origin.x),
        y: f64::from(bounds.origin.y),
        w: f64::from(bounds.size.width),
        h: f64::from(bounds.size.height),
    };

    streemap::squarify(
        streemap_bounds,
        &mut items[..],
        |&(_, size, _)| size,
        |(_, _, item_rect), r| *item_rect = r,
    );

    items
        .iter()
        .map(|(idx, _, item_rect)| {
            let node = &nodes[*idx];
            let node_bounds = Bounds::new(
                Point::new(px(item_rect.x as f32), px(item_rect.y as f32)),
                Size {
                    width: px(item_rect.w as f32),
                    height: px(item_rect.h as f32),
                },
            );

            // Recursively layout children directly in the same bounds (no padding)
            let children = if node.is_directory() && !node.children.is_empty() {
                layout_tree(&node.children, node_bounds, 1.0)
            } else {
                Vec::new()
            };

            LayoutNode {
                id: node.id,
                name: node.name.clone(),
                bounds: node_bounds,
                children,
            }
        })
        .collect()
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
