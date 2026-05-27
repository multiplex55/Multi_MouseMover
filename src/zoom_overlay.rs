use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurgicalZoomConfig {
    pub enabled: bool,
    pub zoom_enabled: bool,
    pub zoom_scale: f32,
    pub zoom_size_px: i32,
    pub overlay_offset_x: i32,
    pub overlay_offset_y: i32,
    pub refresh_interval_ms: u64,
    pub center_crosshair: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurgicalZoomState {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub source_left: i32,
    pub source_top: i32,
    pub source_size_px: i32,
    pub center_crosshair: bool,
    pub pending_refresh: bool,
}

impl Default for SurgicalZoomState {
    fn default() -> Self {
        Self {
            visible: false,
            x: 0,
            y: 0,
            source_left: 0,
            source_top: 0,
            source_size_px: 0,
            center_crosshair: false,
            pending_refresh: false,
        }
    }
}

pub fn update_zoom_state(
    state: &mut SurgicalZoomState,
    config: SurgicalZoomConfig,
    surgical_active: bool,
    cursor_x: i32,
    cursor_y: i32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: i32,
    virtual_height: i32,
) {
    if !(config.enabled && config.zoom_enabled && surgical_active) {
        state.visible = false;
        state.pending_refresh = false;
        return;
    }

    state.visible = true;
    state.x = cursor_x + config.overlay_offset_x;
    state.y = cursor_y + config.overlay_offset_y;
    let source_size = ((config.zoom_size_px as f32) / config.zoom_scale).round().max(1.0) as i32;
    let (source_left, source_top) = surgical_zoom_source_rect(cursor_x, cursor_y, source_size, virtual_left, virtual_top, virtual_width, virtual_height);
    state.source_left = source_left;
    state.source_top = source_top;
    state.source_size_px = source_size;
    state.center_crosshair = config.center_crosshair;
    state.pending_refresh = true;
}

pub fn surgical_zoom_source_rect(
    cursor_x: i32,
    cursor_y: i32,
    source_size_px: i32,
    virtual_left: i32,
    virtual_top: i32,
    virtual_width: i32,
    virtual_height: i32,
) -> (i32, i32) {
    let half = source_size_px / 2;
    let source_width = source_size_px.min(virtual_width.max(1));
    let source_height = source_size_px.min(virtual_height.max(1));
    let max_left = virtual_left + virtual_width - source_width;
    let max_top = virtual_top + virtual_height - source_height;
    (
        (cursor_x - half).clamp(virtual_left, max_left),
        (cursor_y - half).clamp(virtual_top, max_top),
    )
}

pub fn zoom_refresh_throttles_to_configured_interval(
    last_refresh: Option<Instant>,
    now: Instant,
    refresh_interval_ms: u64,
) -> bool {
    if refresh_interval_ms == 0 {
        return true;
    }
    let interval = Duration::from_millis(refresh_interval_ms);
    last_refresh.is_none_or(|ts| now.duration_since(ts) >= interval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surgical_zoom_source_rect_clamps_at_screen_edges() {
        assert_eq!(surgical_zoom_source_rect(2, 2, 100, 0, 0, 1920, 1080), (0, 0));
        assert_eq!(surgical_zoom_source_rect(1919, 1079, 100, 0, 0, 1920, 1080), (1820, 980));
        assert_eq!(surgical_zoom_source_rect(-1900, -100, 120, -1920, -200, 3840, 2160), (-1920, -160));
    }

    #[test]
    fn overlay_position_applies_configured_offsets() {
        let mut state = SurgicalZoomState::default();
        let cfg = SurgicalZoomConfig {
            enabled: true,
            zoom_enabled: true,
            zoom_scale: 2.0,
            zoom_size_px: 180,
            overlay_offset_x: 24,
            overlay_offset_y: 12,
            refresh_interval_ms: 16,
            center_crosshair: true,
        };
        update_zoom_state(&mut state, cfg, true, 100, 200, 0, 0, 1920, 1080);
        assert_eq!((state.x, state.y), (124, 212));
    }

    #[test]
    fn surgical_mode_visibility_toggles_overlay_state() {
        let mut state = SurgicalZoomState::default();
        let cfg = SurgicalZoomConfig { enabled: true, zoom_enabled: true, zoom_scale: 2.0, zoom_size_px: 180, overlay_offset_x: 0, overlay_offset_y: 0, refresh_interval_ms: 16, center_crosshair: false };
        update_zoom_state(&mut state, cfg, true, 10, 20, 0, 0, 100, 100);
        assert!(state.visible);
        update_zoom_state(&mut state, cfg, false, 10, 20, 0, 0, 100, 100);
        assert!(!state.visible);
    }

    #[test]
    fn zoom_refresh_throttles_to_configured_interval() {
        let now = Instant::now();
        assert!(zoom_refresh_throttles_to_configured_interval(None, now, 16));
        assert!(!zoom_refresh_throttles_to_configured_interval(Some(now), now + Duration::from_millis(8), 16));
        assert!(zoom_refresh_throttles_to_configured_interval(Some(now), now + Duration::from_millis(16), 16));
    }
}
