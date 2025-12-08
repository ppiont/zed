use gpui::{px, Bounds, Pixels, Point, Size};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::data::NodeId;

const ANIMATION_DURATION: Duration = Duration::from_millis(300);

/// Tracks animated bounds for each node
#[derive(Default)]
pub struct AnimationState {
    /// Previous bounds per node (starting point of animation)
    previous_bounds: HashMap<NodeId, Bounds<Pixels>>,
    /// Target bounds per node (end point of animation)
    target_bounds: HashMap<NodeId, Bounds<Pixels>>,
    /// Animation start time
    start_time: Option<Instant>,
    /// Whether animation is in progress
    pub animating: bool,
}

impl AnimationState {
    /// Check if we have initial bounds set
    pub fn has_initial_bounds(&self) -> bool {
        !self.target_bounds.is_empty()
    }

    /// Set initial bounds without animating (used for first render)
    pub fn set_initial_bounds(&mut self, bounds: HashMap<NodeId, Bounds<Pixels>>) {
        if self.target_bounds.is_empty() {
            self.target_bounds = bounds;
            self.previous_bounds = self.target_bounds.clone();
        }
    }

    /// Start animating to new target bounds
    pub fn animate_to(&mut self, new_targets: HashMap<NodeId, Bounds<Pixels>>) {
        // If we have no previous bounds, this is effectively the first real layout
        // Use current targets as the starting point for animation
        if self.target_bounds.is_empty() {
            // No animation on first load - just set targets
            self.target_bounds = new_targets;
            self.previous_bounds = self.target_bounds.clone();
            return;
        }

        // Save current target positions as previous (starting point)
        for (id, target) in &new_targets {
            if let Some(current_target) = self.target_bounds.get(id) {
                self.previous_bounds.insert(*id, *current_target);
            } else {
                // New node - start from target (no animation for new nodes)
                self.previous_bounds.insert(*id, *target);
            }
        }

        // Remove nodes that no longer exist
        self.previous_bounds
            .retain(|id, _| new_targets.contains_key(id));

        self.target_bounds = new_targets;
        self.start_time = Some(Instant::now());
        self.animating = true;
    }

    /// Update animation state, returns true if still animating
    pub fn update(&mut self) -> bool {
        let Some(start) = self.start_time else {
            return false;
        };

        let elapsed = start.elapsed();
        if elapsed >= ANIMATION_DURATION {
            // Animation complete - set previous to target
            self.previous_bounds = self.target_bounds.clone();
            self.start_time = None;
            self.animating = false;
            return false;
        }

        true
    }

    /// Get current progress (0.0 to 1.0, eased)
    fn progress(&self) -> f32 {
        let Some(start) = self.start_time else {
            return 1.0;
        };

        let elapsed = start.elapsed();
        let t = (elapsed.as_secs_f32() / ANIMATION_DURATION.as_secs_f32()).min(1.0);
        ease_out_cubic(t)
    }

    /// Get current (possibly animated) bounds for a node
    pub fn get_bounds(&self, id: NodeId) -> Option<Bounds<Pixels>> {
        let target = self.target_bounds.get(&id)?;

        if !self.animating {
            return Some(*target);
        }

        let previous = self.previous_bounds.get(&id).unwrap_or(target);
        let t = self.progress();

        Some(interpolate_bounds(*previous, *target, t))
    }

    /// Check if we have bounds for a node
    #[allow(dead_code)]
    pub fn has_node(&self, id: NodeId) -> bool {
        self.target_bounds.contains_key(&id)
    }

    /// Get target bounds directly (for layout calculations)
    #[allow(dead_code)]
    pub fn get_target_bounds(&self, id: NodeId) -> Option<Bounds<Pixels>> {
        self.target_bounds.get(&id).copied()
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn interpolate_bounds(from: Bounds<Pixels>, to: Bounds<Pixels>, t: f32) -> Bounds<Pixels> {
    Bounds::new(
        Point::new(
            px(f32::from(from.origin.x) + (f32::from(to.origin.x) - f32::from(from.origin.x)) * t),
            px(f32::from(from.origin.y) + (f32::from(to.origin.y) - f32::from(from.origin.y)) * t),
        ),
        Size {
            width: px(
                f32::from(from.size.width)
                    + (f32::from(to.size.width) - f32::from(from.size.width)) * t,
            ),
            height: px(
                f32::from(from.size.height)
                    + (f32::from(to.size.height) - f32::from(from.size.height)) * t,
            ),
        },
    )
}
