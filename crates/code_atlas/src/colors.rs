use gpui::Hsla;
use theme::ActiveTheme;

/// Calculates color based on days since last change
pub fn activity_color(timestamp: Option<i64>, cx: &impl ActiveTheme) -> Hsla {
    let theme = cx.theme();

    let Some(timestamp) = timestamp else {
        // No git info - use neutral color
        return theme.colors().element_background;
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let seconds_ago = (now - timestamp).max(0);
    // Convert to days
    let days_ago = seconds_ago as f64 / 86400.0;

    if days_ago < 1.0 {
         // Hot: changed today - bright red
        interpolate_color(
            theme.status().error,
            theme.status().warning,
            days_ago as f32, // 0 to 1
        )
    } else if days_ago < 7.0 {
        // Warm: this week - orange to yellow
        interpolate_color(
            theme.status().warning,
            theme.status().info,
            ((days_ago - 1.0) / 6.0) as f32,
        )
    } else if days_ago < 30.0 {
        // Cool: this month - yellow to muted
        interpolate_color(
            theme.status().info,
            theme.colors().text_muted,
            ((days_ago - 7.0) / 23.0) as f32,
        )
    } else {
        // Cold: older than a month - muted to disabled
        let t = ((days_ago - 30.0) / 60.0).min(1.0) as f32;
        interpolate_color(
            theme.colors().text_muted,
            theme.colors().text_disabled,
            t,
        )
    }
}

fn interpolate_color(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    Hsla {
        h: from.h + (to.h - from.h) * t,
        s: from.s + (to.s - from.s) * t,
        l: from.l + (to.l - from.l) * t,
        a: from.a + (to.a - from.a) * t,
    }
}
