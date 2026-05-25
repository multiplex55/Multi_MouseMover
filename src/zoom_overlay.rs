#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurgicalZoomConfig {
    pub enabled: bool,
    pub zoom_enabled: bool,
    pub zoom_scale: f32,
    pub zoom_size_px: i32,
    pub overlay_offset_x: i32,
    pub overlay_offset_y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurgicalZoomState {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
}

impl Default for SurgicalZoomState {
    fn default() -> Self {
        Self { visible: false, x: 0, y: 0 }
    }
}

pub fn update_zoom_state(
    state: &mut SurgicalZoomState,
    config: SurgicalZoomConfig,
    surgical_active: bool,
    cursor_x: i32,
    cursor_y: i32,
) {
    if !(config.enabled && config.zoom_enabled && surgical_active) {
        state.visible = false;
        return;
    }

    state.visible = true;
    state.x = cursor_x + config.overlay_offset_x;
    state.y = cursor_y + config.overlay_offset_y;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_and_deactivation_toggle_visibility() {
        let mut state = SurgicalZoomState::default();
        let cfg = SurgicalZoomConfig {
            enabled: true,
            zoom_enabled: true,
            zoom_scale: 2.0,
            zoom_size_px: 180,
            overlay_offset_x: 24,
            overlay_offset_y: 24,
        };

        update_zoom_state(&mut state, cfg, true, 100, 200);
        assert!(state.visible);
        assert_eq!((state.x, state.y), (124, 224));

        update_zoom_state(&mut state, cfg, false, 100, 200);
        assert!(!state.visible);
    }
}
