use gpui::{point, px, Bounds, Pixels, Point, Size};

/// Manages pan/zoom state and hit testing
pub struct InteractionState {
    /// Current pan offset (screen coordinates)
    pub pan_offset: Point<Pixels>,
    /// Current zoom level (1.0 = 100%)
    pub zoom: f32,
    /// Minimum zoom level
    pub min_zoom: f32,
    /// Maximum zoom level
    pub max_zoom: f32,
    /// Is user currently dragging to pan
    pub is_panning: bool,
    /// Last mouse position during pan
    pub last_pan_position: Point<Pixels>,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            pan_offset: point(px(0.), px(0.)),
            zoom: 1.0,
            min_zoom: 0.1,
            max_zoom: 10.0,
            is_panning: false,
            last_pan_position: point(px(0.), px(0.)),
        }
    }
}

impl InteractionState {
    /// Convert screen coordinates to world coordinates
    #[allow(dead_code)]
    pub fn screen_to_world(&self, screen_pos: Point<Pixels>) -> Point<Pixels> {
        let zoom = self.zoom;
        let screen_x: f32 = screen_pos.x.into();
        let screen_y: f32 = screen_pos.y.into();
        let pan_x: f32 = self.pan_offset.x.into();
        let pan_y: f32 = self.pan_offset.y.into();
        point(px((screen_x - pan_x) / zoom), px((screen_y - pan_y) / zoom))
    }

    /// Convert world coordinates to screen coordinates
    #[allow(dead_code)]
    pub fn world_to_screen(&self, world_pos: Point<Pixels>) -> Point<Pixels> {
        let zoom = self.zoom;
        let world_x: f32 = world_pos.x.into();
        let world_y: f32 = world_pos.y.into();
        let pan_x: f32 = self.pan_offset.x.into();
        let pan_y: f32 = self.pan_offset.y.into();
        point(px(world_x * zoom + pan_x), px(world_y * zoom + pan_y))
    }

    /// Transform bounds from world to screen coordinates
    #[allow(dead_code)]
    pub fn transform_bounds(&self, world_bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let zoom = self.zoom;
        let origin_x: f32 = world_bounds.origin.x.into();
        let origin_y: f32 = world_bounds.origin.y.into();
        let width: f32 = world_bounds.size.width.into();
        let height: f32 = world_bounds.size.height.into();
        let pan_x: f32 = self.pan_offset.x.into();
        let pan_y: f32 = self.pan_offset.y.into();
        Bounds::new(
            point(px(origin_x * zoom + pan_x), px(origin_y * zoom + pan_y)),
            Size {
                width: px(width * zoom),
                height: px(height * zoom),
            },
        )
    }

    /// Apply zoom centered on a screen position
    pub fn zoom_at(&mut self, screen_pos: Point<Pixels>, delta: f32) {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * (1.0 + delta)).clamp(self.min_zoom, self.max_zoom);

        if (self.zoom - old_zoom).abs() > f32::EPSILON {
            let zoom_ratio = self.zoom / old_zoom;
            let pan_x: f32 = self.pan_offset.x.into();
            let pan_y: f32 = self.pan_offset.y.into();
            let screen_x: f32 = screen_pos.x.into();
            let screen_y: f32 = screen_pos.y.into();
            self.pan_offset = point(
                px(screen_x - (screen_x - pan_x) * zoom_ratio),
                px(screen_y - (screen_y - pan_y) * zoom_ratio),
            );
        }
    }

    /// Start panning from a position
    pub fn start_pan(&mut self, position: Point<Pixels>) {
        self.is_panning = true;
        self.last_pan_position = position;
    }

    /// Update pan with new mouse position
    pub fn update_pan(&mut self, position: Point<Pixels>) {
        if self.is_panning {
            let delta_x = position.x - self.last_pan_position.x;
            let delta_y = position.y - self.last_pan_position.y;
            self.pan_offset = point(self.pan_offset.x + delta_x, self.pan_offset.y + delta_y);
            self.last_pan_position = position;
        }
    }

    /// Stop panning
    pub fn stop_pan(&mut self) {
        self.is_panning = false;
    }
}
