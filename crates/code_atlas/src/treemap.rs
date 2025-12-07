use gpui::{px, Bounds, Pixels, Point, Size};
use std::cmp::Ordering;

/// A node in the treemap with its computed bounds
#[derive(Clone, Debug)]
pub struct LayoutNode {
    pub id: usize,
    #[allow(dead_code)]
    pub size: f64,
    pub bounds: Bounds<Pixels>,
}

/// Computes squarified treemap layout for a list of sizes
///
/// Algorithm: Bruls, Huizing, van Wijk (2000) "Squarified Treemaps"
/// https://www.win.tue.nl/~vanwijk/stm.pdf
pub fn squarify(sizes: &[(usize, f64)], bounds: Bounds<Pixels>) -> Vec<LayoutNode> {
    if sizes.is_empty() {
        return Vec::new();
    }

    // Sort by size descending (required for squarify algorithm)
    let mut sorted: Vec<_> = sizes.iter().copied().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    let total_size: f64 = sorted.iter().map(|(_, s)| s).sum();
    if total_size <= 0.0 {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(sorted.len());
    let mut remaining = sorted.as_slice();
    let mut current_bounds = bounds;

    while !remaining.is_empty() {
        let (row, rest) = layout_row(remaining, current_bounds, total_size);

        // Calculate row bounds and update remaining area
        let row_size: f64 = row.iter().map(|(_, s)| s).sum();
        let row_fraction = row_size / total_size;

        let (row_bounds, new_bounds) = split_bounds(current_bounds, row_fraction);

        // Layout items within the row
        let mut offset = 0.0;
        let is_horizontal = row_bounds.size.width >= row_bounds.size.height;

        for (id, size) in row {
            let item_fraction = size / row_size;
            let item_bounds = if is_horizontal {
                let width = row_bounds.size.width.to_f64() * item_fraction;
                Bounds::new(
                    Point::new(
                        row_bounds.origin.x + px(offset as f32),
                        row_bounds.origin.y,
                    ),
                    Size {
                        width: px(width as f32),
                        height: row_bounds.size.height,
                    },
                )
            } else {
                let height = row_bounds.size.height.to_f64() * item_fraction;
                Bounds::new(
                    Point::new(
                        row_bounds.origin.x,
                        row_bounds.origin.y + px(offset as f32),
                    ),
                    Size {
                        width: row_bounds.size.width,
                        height: px(height as f32),
                    },
                )
            };

            if is_horizontal {
                offset += row_bounds.size.width.to_f64() * item_fraction;
            } else {
                offset += row_bounds.size.height.to_f64() * item_fraction;
            }

            result.push(LayoutNode {
                id: *id,
                size: *size,
                bounds: item_bounds,
            });
        }

        remaining = rest;
        current_bounds = new_bounds;
    }

    result
}

/// Determines how many items to include in the current row
fn layout_row(
    items: &[(usize, f64)],
    bounds: Bounds<Pixels>,
    total_size: f64,
) -> (&[(usize, f64)], &[(usize, f64)]) {
    if items.len() == 1 {
        return (items, &[]);
    }

    let width = bounds
        .size
        .width
        .to_f64()
        .min(bounds.size.height.to_f64());
    if width <= 0.0 {
        return (items, &[]);
    }

    let mut best_ratio = f64::MAX;
    let mut best_count = 1;

    for count in 1..=items.len() {
        let row_items = &items[..count];
        let row_size: f64 = row_items.iter().map(|(_, s)| s).sum();
        let row_area =
            (row_size / total_size) * (bounds.size.width.to_f64() * bounds.size.height.to_f64());
        let row_width = row_area / width;

        // Calculate worst aspect ratio in this row
        let mut worst_ratio = 0.0f64;
        for (_, size) in row_items {
            let item_height = (size / row_size) * width;
            let ratio = if row_width > item_height {
                row_width / item_height
            } else {
                item_height / row_width
            };
            worst_ratio = worst_ratio.max(ratio);
        }

        if worst_ratio <= best_ratio {
            best_ratio = worst_ratio;
            best_count = count;
        } else {
            break;
        }
    }

    (&items[..best_count], &items[best_count..])
}

/// Splits bounds into row area and remaining area
fn split_bounds(bounds: Bounds<Pixels>, fraction: f64) -> (Bounds<Pixels>, Bounds<Pixels>) {
    let is_horizontal = bounds.size.width >= bounds.size.height;

    if is_horizontal {
        let height = bounds.size.height.to_f64() * fraction;
        let row = Bounds::new(
            bounds.origin,
            Size {
                width: bounds.size.width,
                height: px(height as f32),
            },
        );
        let remaining = Bounds::new(
            Point::new(bounds.origin.x, bounds.origin.y + px(height as f32)),
            Size {
                width: bounds.size.width,
                height: px((bounds.size.height.to_f64() * (1.0 - fraction)) as f32),
            },
        );
        (row, remaining)
    } else {
        let width = bounds.size.width.to_f64() * fraction;
        let row = Bounds::new(
            bounds.origin,
            Size {
                width: px(width as f32),
                height: bounds.size.height,
            },
        );
        let remaining = Bounds::new(
            Point::new(bounds.origin.x + px(width as f32), bounds.origin.y),
            Size {
                width: px((bounds.size.width.to_f64() * (1.0 - fraction)) as f32),
                height: bounds.size.height,
            },
        );
        (row, remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_squarify_empty() {
        let result = squarify(&[], Bounds::new(Point::default(), Size::default()));
        assert!(result.is_empty());
    }

    #[test]
    fn test_squarify_single() {
        let sizes = vec![(0, 100.0)];
        let bounds = Bounds::new(
            Point::new(px(0.), px(0.)),
            Size {
                width: px(100.),
                height: px(100.),
            },
        );
        let result = squarify(&sizes, bounds);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 0);
    }

    #[test]
    fn test_squarify_multiple() {
        let sizes = vec![
            (0, 6.0),
            (1, 6.0),
            (2, 4.0),
            (3, 3.0),
            (4, 2.0),
            (5, 1.0),
        ];
        let bounds = Bounds::new(
            Point::new(px(0.), px(0.)),
            Size {
                width: px(100.),
                height: px(100.),
            },
        );
        let result = squarify(&sizes, bounds);
        assert_eq!(result.len(), 6);

        // Verify all nodes have positive area
        for node in &result {
            assert!(node.bounds.size.width.to_f64() > 0.0);
            assert!(node.bounds.size.height.to_f64() > 0.0);
        }
    }
}
