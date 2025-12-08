use gpui::{Bounds, Pixels};

const MIN_LABEL_SIZE: f32 = 50.0;
const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 14.0;

/// Determines if a label should be shown based on screen size
pub fn should_show_label(screen_bounds: Bounds<Pixels>) -> bool {
    let width: f32 = screen_bounds.size.width.into();
    let height: f32 = screen_bounds.size.height.into();
    width >= MIN_LABEL_SIZE && height >= MIN_LABEL_SIZE
}

/// Calculates label font size based on rectangle size
pub fn label_font_size(screen_bounds: Bounds<Pixels>) -> f32 {
    let width: f32 = screen_bounds.size.width.into();
    let height: f32 = screen_bounds.size.height.into();
    let min_dim = width.min(height);
    (min_dim / 6.0).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

/// Calculates label opacity based on rectangle size (smooth fade in)
pub fn label_opacity(screen_bounds: Bounds<Pixels>) -> f32 {
    let width: f32 = screen_bounds.size.width.into();
    let height: f32 = screen_bounds.size.height.into();
    let min_dim = width.min(height);
    if min_dim < MIN_LABEL_SIZE {
        0.0
    } else if min_dim < MIN_LABEL_SIZE + 20.0 {
        (min_dim - MIN_LABEL_SIZE) / 20.0
    } else {
        1.0
    }
}
