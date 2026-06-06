use crate::bookmarks::{BookmarkRecord, BookmarkStore};
use crate::keyboard::KeyBindings;
use crate::{BookmarkCoordinatePolicy, BookmarkMarkerPositionSource, BookmarkMarkerShape, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualScreenRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

pub type Rect = VirtualScreenRect;

#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkMarkerOverlayView {
    pub markers: Vec<BookmarkMarkerView>,
    pub shape: BookmarkMarkerShape,
    pub size_px: i32,
    pub border_width_px: i32,
    pub font_scale: f32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub center_on_bookmark: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkMarkerView {
    pub slot: u8,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub fill_color: RgbColor,
    pub text_color: RgbColor,
    pub border_color: RgbColor,
    pub opacity: f32,
}

pub fn point_inside_virtual_screen(x: i32, y: i32, screen: VirtualScreenRect) -> bool {
    x >= screen.left
        && x < screen.left + screen.width
        && y >= screen.top
        && y < screen.top + screen.height
}

pub fn build_bookmark_marker_overlay_view(
    config: &Config,
    store: &BookmarkStore,
    key_bindings: &KeyBindings,
    current_desktop_id: Option<&str>,
    virtual_screen: Rect,
    desktop_checker: impl Fn(&BookmarkRecord) -> bool,
) -> BookmarkMarkerOverlayView {
    let marker_config = &config.bookmark_markers;
    let global_fill = parse_rgb_color(&marker_config.fill_color).unwrap_or(default_fill_color());
    let global_text = parse_rgb_color(&marker_config.text_color).unwrap_or(default_text_color());
    let global_border =
        parse_rgb_color(&marker_config.border_color).unwrap_or(default_border_color());
    let global_opacity = normalize_opacity(marker_config.opacity);

    let mut markers = Vec::new();
    for slot in 1..=(config.bookmarks.slot_count as u8) {
        let Some(record) = store.get_slot(slot) else {
            continue;
        };
        let Some(label) = key_bindings.display_for_bookmark_slot(slot) else {
            continue;
        };

        if marker_config.filter_current_virtual_desktop {
            let Some(current_desktop_id) = current_desktop_id else {
                continue;
            };
            if record.virtual_desktop_id.as_deref() != Some(current_desktop_id) {
                continue;
            }
            if !desktop_checker(record) {
                continue;
            }
        }

        let (x, y) = resolve_marker_position(record, config, virtual_screen);
        if marker_config.hide_offscreen && !point_inside_virtual_screen(x, y, virtual_screen) {
            continue;
        }

        let (fill_color, text_color, border_color, opacity) = config
            .bookmark_markers
            .slot_styles
            .get(&slot.to_string())
            .map(|style| {
                (
                    parse_rgb_color(&style.fill_color).unwrap_or(global_fill),
                    parse_rgb_color(&style.text_color).unwrap_or(global_text),
                    parse_rgb_color(&style.border_color).unwrap_or(global_border),
                    normalize_opacity(style.opacity),
                )
            })
            .unwrap_or((global_fill, global_text, global_border, global_opacity));

        markers.push(BookmarkMarkerView {
            slot,
            label,
            x,
            y,
            fill_color,
            text_color,
            border_color,
            opacity,
        });
    }

    BookmarkMarkerOverlayView {
        markers,
        shape: marker_config.shape,
        size_px: normalize_size_px(marker_config.size_px),
        border_width_px: normalize_border_width_px(marker_config.border_width_px),
        font_scale: normalize_font_scale(marker_config.font_scale),
        offset_x: marker_config.offset_x.clamp(-200, 200),
        offset_y: marker_config.offset_y.clamp(-200, 200),
        center_on_bookmark: marker_config.center_on_bookmark,
    }
}

fn resolve_marker_position(
    record: &BookmarkRecord,
    config: &Config,
    virtual_screen: VirtualScreenRect,
) -> (i32, i32) {
    match config.bookmark_markers.position_source {
        BookmarkMarkerPositionSource::SavedCoordinate => (record.x, record.y),
        BookmarkMarkerPositionSource::ResolvedRecallTarget => {
            resolve_recall_target_for_virtual_screen(
                record,
                config.bookmarks.coordinate_policy,
                virtual_screen,
            )
        }
    }
}

fn resolve_recall_target_for_virtual_screen(
    record: &BookmarkRecord,
    policy: BookmarkCoordinatePolicy,
    virtual_screen: VirtualScreenRect,
) -> (i32, i32) {
    let (mut x, mut y) = (record.x, record.y);
    match policy {
        BookmarkCoordinatePolicy::Exact => {}
        BookmarkCoordinatePolicy::ClampToNearestMonitor => {
            let mr = &record.monitor_rect;
            x = x.clamp(mr.left, mr.right.saturating_sub(1));
            y = y.clamp(mr.top, mr.bottom.saturating_sub(1));
        }
        BookmarkCoordinatePolicy::ClampToVirtualScreen => {
            x = x.clamp(
                virtual_screen.left,
                (virtual_screen.left + virtual_screen.width).saturating_sub(1),
            );
            y = y.clamp(
                virtual_screen.top,
                (virtual_screen.top + virtual_screen.height).saturating_sub(1),
            );
        }
    }
    (x, y)
}

fn parse_rgb_color(value: &str) -> Option<RgbColor> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(RgbColor {
        red: u8::from_str_radix(&hex[0..2], 16).ok()?,
        green: u8::from_str_radix(&hex[2..4], 16).ok()?,
        blue: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
}

fn default_fill_color() -> RgbColor {
    RgbColor {
        red: 0xFF,
        green: 0xD4,
        blue: 0x00,
    }
}

fn default_text_color() -> RgbColor {
    RgbColor {
        red: 0,
        green: 0,
        blue: 0,
    }
}

fn default_border_color() -> RgbColor {
    default_text_color()
}

fn normalize_opacity(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.10, 1.00)
    } else {
        0.82
    }
}

fn normalize_size_px(value: i32) -> i32 {
    value.clamp(12, 96)
}

fn normalize_border_width_px(value: i32) -> i32 {
    value.clamp(0, 8)
}

fn normalize_font_scale(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.50, 3.00)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::bookmarks::MonitorRect as BookmarkMonitorRect;
    use crate::key_chord::KeyChord;
    use crate::keyboard::VirtualKey;
    use crate::{BookmarkMarkerSlotStyle, BookmarksConfig};

    fn screen() -> VirtualScreenRect {
        VirtualScreenRect {
            left: -100,
            top: 0,
            width: 300,
            height: 100,
        }
    }

    fn config() -> Config {
        let mut config = Config::default();
        config.bookmarks.slot_count = 9;
        config.bookmark_markers.slot_styles.clear();
        config.bookmark_markers.filter_current_virtual_desktop = false;
        config.bookmark_markers.hide_offscreen = false;
        config
    }

    fn key_for_slot(slot: u8) -> VirtualKey {
        match slot {
            1 => VirtualKey::Num1,
            2 => VirtualKey::Num2,
            3 => VirtualKey::Num3,
            4 => VirtualKey::Num4,
            5 => VirtualKey::Num5,
            6 => VirtualKey::Num6,
            7 => VirtualKey::Num7,
            8 => VirtualKey::Num8,
            9 => VirtualKey::Num9,
            _ => VirtualKey::Num0,
        }
    }

    fn bindings(slots: &[u8]) -> KeyBindings {
        let mut bindings = KeyBindings::new();
        for slot in slots {
            bindings.add_chord_binding_with_display(
                KeyChord::from_key(key_for_slot(*slot)),
                Action::BookmarkSlot(*slot),
                format!("B{slot}"),
            );
        }
        bindings
    }

    fn record(slot: u8, x: i32, y: i32) -> BookmarkRecord {
        BookmarkRecord {
            slot,
            name: None,
            x,
            y,
            monitor_device_name: "m".into(),
            monitor_rect: BookmarkMonitorRect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            virtual_desktop_id: Some("desktop-a".into()),
            anchor_hwnd: Some(1),
            anchor_process_id: Some(10),
            anchor_window_title: Some("window".into()),
            created_at_unix_ms: 0,
            updated_at_unix_ms: 0,
        }
    }

    fn store_with(records: impl IntoIterator<Item = BookmarkRecord>) -> BookmarkStore {
        let mut store = BookmarkStore::new(9);
        for record in records {
            store.set_slot(record.slot, record);
        }
        store
    }

    fn view(
        config: &Config,
        store: &BookmarkStore,
        bindings: &KeyBindings,
    ) -> BookmarkMarkerOverlayView {
        build_bookmark_marker_overlay_view(
            config,
            store,
            bindings,
            Some("desktop-a"),
            screen(),
            |_| true,
        )
    }

    #[test]
    fn only_active_saved_slots_are_shown() {
        let config = config();
        let store = store_with([record(1, 1, 1), record(3, 3, 3)]);
        let bindings = bindings(&[1, 2, 3]);

        assert_eq!(
            view(&config, &store, &bindings)
                .markers
                .iter()
                .map(|marker| marker.slot)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn cleared_slot_is_removed_from_markers() {
        let config = config();
        let mut store = store_with([record(1, 1, 1), record(2, 2, 2)]);
        store.remove_slot(1);
        let bindings = bindings(&[1, 2]);

        assert_eq!(view(&config, &store, &bindings).markers[0].slot, 2);
    }

    #[test]
    fn bookmark_names_never_appear_in_marker_labels() {
        let config = config();
        let mut named = record(1, 1, 1);
        named.name = Some("Secret Name".into());
        let store = store_with([named]);
        let mut bindings = KeyBindings::new();
        bindings.add_chord_binding_with_display(
            KeyChord::parse("1").unwrap(),
            Action::BookmarkSlot(1),
            "1",
        );

        let marker_view = view(&config, &store, &bindings);
        let labels = marker_view
            .markers
            .iter()
            .map(|marker| marker.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"1"));
        assert!(!labels.contains(&"Secret Name"));
        assert!(labels.iter().all(|label| !label.contains("Secret Name")));
    }

    #[test]
    fn rebuilding_marker_view_after_keybinding_change_updates_label() {
        let config = config();
        let mut named = record(1, 1, 1);
        named.name = Some("Secret Name".into());
        let store = store_with([named]);
        let mut initial_bindings = KeyBindings::new();
        initial_bindings.add_chord_binding_with_display(
            KeyChord::parse("1").unwrap(),
            Action::BookmarkSlot(1),
            "1",
        );
        let mut updated_bindings = KeyBindings::new();
        updated_bindings.add_chord_binding_with_display(
            KeyChord::parse("RightAlt+1").unwrap(),
            Action::BookmarkSlot(1),
            "RightAlt+1",
        );

        assert_eq!(
            view(&config, &store, &initial_bindings).markers[0].label,
            "1"
        );
        assert_eq!(
            view(&config, &store, &updated_bindings).markers[0].label,
            "RightAlt+1"
        );
    }

    #[test]
    fn slot_without_display_binding_is_hidden() {
        let config = config();
        let store = store_with([record(1, 1, 1), record(2, 2, 2)]);
        let bindings = bindings(&[2]);

        assert_eq!(
            view(&config, &store, &bindings)
                .markers
                .iter()
                .map(|marker| marker.slot)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn missing_current_desktop_is_hidden_when_strict_filtering_is_enabled() {
        let mut config = config();
        config.bookmark_markers.filter_current_virtual_desktop = true;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        let view =
            build_bookmark_marker_overlay_view(&config, &store, &bindings, None, screen(), |_| {
                true
            });

        assert!(view.markers.is_empty());
    }

    #[test]
    fn current_desktop_match_is_shown() {
        let mut config = config();
        config.bookmark_markers.filter_current_virtual_desktop = true;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        assert_eq!(view(&config, &store, &bindings).markers.len(), 1);
    }

    #[test]
    fn desktop_mismatch_is_hidden() {
        let mut config = config();
        config.bookmark_markers.filter_current_virtual_desktop = true;
        let mut other = record(1, 1, 1);
        other.virtual_desktop_id = Some("desktop-b".into());
        let store = store_with([other]);
        let bindings = bindings(&[1]);

        assert!(view(&config, &store, &bindings).markers.is_empty());
    }

    #[test]
    fn missing_desktop_id_is_hidden_when_strict_filtering_is_enabled() {
        let mut config = config();
        config.bookmark_markers.filter_current_virtual_desktop = true;
        let mut missing = record(1, 1, 1);
        missing.virtual_desktop_id = None;
        let store = store_with([missing]);
        let bindings = bindings(&[1]);

        assert!(view(&config, &store, &bindings).markers.is_empty());
    }

    #[test]
    fn missing_invalid_not_current_anchor_is_hidden_via_desktop_checker() {
        let mut config = config();
        config.bookmark_markers.filter_current_virtual_desktop = true;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        let view = build_bookmark_marker_overlay_view(
            &config,
            &store,
            &bindings,
            Some("desktop-a"),
            screen(),
            |_| false,
        );

        assert!(view.markers.is_empty());
    }

    #[test]
    fn offscreen_marker_is_hidden_when_hide_offscreen_is_true() {
        let mut config = config();
        config.bookmark_markers.hide_offscreen = true;
        let store = store_with([record(1, 500, 50)]);
        let bindings = bindings(&[1]);

        assert!(view(&config, &store, &bindings).markers.is_empty());
    }

    #[test]
    fn points_on_different_monitors_within_the_virtual_screen_are_allowed() {
        let mut config = config();
        config.bookmark_markers.hide_offscreen = true;
        let store = store_with([record(1, -50, 50), record(2, 150, 50)]);
        let bindings = bindings(&[1, 2]);

        assert_eq!(view(&config, &store, &bindings).markers.len(), 2);
    }

    #[test]
    fn global_default_style_is_applied() {
        let mut config = config();
        config.bookmark_markers.fill_color = "#112233".into();
        config.bookmark_markers.text_color = "#445566".into();
        config.bookmark_markers.border_color = "#778899".into();
        config.bookmark_markers.opacity = 0.7;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);
        let marker = &view(&config, &store, &bindings).markers[0];

        assert_eq!(
            marker.fill_color,
            RgbColor {
                red: 0x11,
                green: 0x22,
                blue: 0x33
            }
        );
        assert_eq!(
            marker.text_color,
            RgbColor {
                red: 0x44,
                green: 0x55,
                blue: 0x66
            }
        );
        assert_eq!(
            marker.border_color,
            RgbColor {
                red: 0x77,
                green: 0x88,
                blue: 0x99
            }
        );
        assert_eq!(marker.opacity, 0.7);
    }

    #[test]
    fn per_slot_fill_text_border_opacity_overrides_are_applied() {
        let mut config = config();
        config.bookmark_markers.slot_styles.insert(
            "1".into(),
            BookmarkMarkerSlotStyle {
                fill_color: "#010203".into(),
                text_color: "#040506".into(),
                border_color: "#070809".into(),
                opacity: 0.5,
            },
        );
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);
        let marker = &view(&config, &store, &bindings).markers[0];

        assert_eq!(
            marker.fill_color,
            RgbColor {
                red: 1,
                green: 2,
                blue: 3
            }
        );
        assert_eq!(
            marker.text_color,
            RgbColor {
                red: 4,
                green: 5,
                blue: 6
            }
        );
        assert_eq!(
            marker.border_color,
            RgbColor {
                red: 7,
                green: 8,
                blue: 9
            }
        );
        assert_eq!(marker.opacity, 0.5);
    }

    #[test]
    fn invalid_colors_fallback() {
        let mut config = config();
        config.bookmark_markers.fill_color = "not-a-color".into();
        config.bookmark_markers.text_color = "#bad".into();
        config.bookmark_markers.border_color = "#GGGGGG".into();
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);
        let marker = &view(&config, &store, &bindings).markers[0];

        assert_eq!(marker.fill_color, default_fill_color());
        assert_eq!(marker.text_color, default_text_color());
        assert_eq!(marker.border_color, default_border_color());
    }

    #[test]
    fn opacity_clamps() {
        assert_eq!(normalize_opacity(-1.0), 0.10);
        assert_eq!(normalize_opacity(2.0), 1.00);
    }

    #[test]
    fn size_clamps() {
        let mut config = config();
        config.bookmark_markers.size_px = 999;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        assert_eq!(view(&config, &store, &bindings).size_px, 96);
    }

    #[test]
    fn border_width_clamps() {
        let mut config = config();
        config.bookmark_markers.border_width_px = -1;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        assert_eq!(view(&config, &store, &bindings).border_width_px, 0);
    }

    #[test]
    fn font_scale_clamps() {
        let mut config = config();
        config.bookmark_markers.font_scale = 10.0;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        assert_eq!(view(&config, &store, &bindings).font_scale, 3.0);
    }

    #[test]
    fn shape_parsing_supports_square_and_circle() {
        let mut square = config();
        square.bookmark_markers.shape = BookmarkMarkerShape::Square;
        let mut circle = config();
        circle.bookmark_markers.shape = BookmarkMarkerShape::Circle;
        let store = store_with([record(1, 1, 1)]);
        let bindings = bindings(&[1]);

        assert_eq!(
            view(&square, &store, &bindings).shape,
            BookmarkMarkerShape::Square
        );
        assert_eq!(
            view(&circle, &store, &bindings).shape,
            BookmarkMarkerShape::Circle
        );
    }

    #[test]
    fn position_source_parsing_supports_saved_coordinate_and_resolved_recall_target() {
        let mut saved = config();
        saved.bookmarks = BookmarksConfig {
            coordinate_policy: BookmarkCoordinatePolicy::ClampToVirtualScreen,
            ..BookmarksConfig::default()
        };
        saved.bookmark_markers.position_source = BookmarkMarkerPositionSource::SavedCoordinate;
        let mut resolved = saved.clone();
        resolved.bookmark_markers.position_source =
            BookmarkMarkerPositionSource::ResolvedRecallTarget;
        resolved.bookmark_markers.hide_offscreen = false;
        let store = store_with([record(1, 500, 50)]);
        let bindings = bindings(&[1]);

        assert_eq!(view(&saved, &store, &bindings).markers[0].x, 500);
        assert_eq!(view(&resolved, &store, &bindings).markers[0].x, 199);
    }
}
