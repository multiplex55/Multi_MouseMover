use std::cell::RefCell;
use std::ptr;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{
    jump_grid::{index_to_code, letters_needed},
    jump_session::{expand_region_within, JumpRegion},
    jump_view::{JumpLabelMetadata, JumpOverlayView, JumpStageMetadata},
    overlay::RGB,
    screen_capture::{capture_virtual_screen, ScreenSnapshot},
    JumpTargetRegionMode, PreviewEdgeBehavior,
};

thread_local! {
    /// Thread-local jump overlay state.
    ///
    /// Win32 window handles (`HWND`) are thread-affine and must be created and
    /// manipulated from the UI/hook thread that owns the window. We intentionally
    /// keep `JumpOverlay` in TLS to prevent accidental cross-thread access.
    pub static JUMP_OVERLAY: RefCell<JumpOverlay> = RefCell::new(JumpOverlay::new());
}

const OVERLAY_ALPHA: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenRect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

pub fn virtual_screen_region() -> JumpRegion {
    ScreenRect::from_virtual_screen().into()
}

impl ScreenRect {
    fn from_virtual_screen() -> Self {
        unsafe {
            Self {
                left: GetSystemMetrics(SM_XVIRTUALSCREEN),
                top: GetSystemMetrics(SM_YVIRTUALSCREEN),
                width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }
}

fn region_to_rect(region: JumpRegion) -> RECT {
    RECT {
        left: region.left,
        top: region.top,
        right: region.left + region.width,
        bottom: region.top + region.height,
    }
}

fn overlay_colorkey() -> COLORREF {
    RGB(0, 0, 0)
}

fn grid_color() -> COLORREF {
    RGB(255, 255, 255)
}

fn label_color() -> COLORREF {
    RGB(255, 255, 0)
}

fn input_color() -> COLORREF {
    RGB(0, 255, 255)
}

fn final_adjust_color() -> COLORREF {
    RGB(255, 64, 64)
}

fn target_outline_color() -> COLORREF {
    RGB(0, 255, 255)
}

fn preview_outline_color() -> COLORREF {
    RGB(96, 96, 96)
}

fn active_grid_outline_color() -> COLORREF {
    RGB(255, 255, 0)
}

fn cell_center_color() -> COLORREF {
    RGB(255, 128, 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransparencyMode {
    ColorKey { color: COLORREF, alpha: u8 },
    Opaque { alpha: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrawMode {
    TransparentGrid,
    MagnifiedPreview,
}

fn overlay_ex_style() -> WINDOW_EX_STYLE {
    WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE
}

fn transparency_mode() -> TransparencyMode {
    TransparencyMode::ColorKey {
        color: overlay_colorkey(),
        alpha: OVERLAY_ALPHA,
    }
}

fn apply_layered_attributes(hwnd: HWND, mode: TransparencyMode) {
    unsafe {
        match mode {
            TransparencyMode::ColorKey { color, alpha } => {
                let _ = SetLayeredWindowAttributes(hwnd, color, alpha, LWA_COLORKEY);
            }
            TransparencyMode::Opaque { alpha } => {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
            }
        }
    }
}

fn draw_mode_for_view(view: &JumpOverlayView) -> DrawMode {
    if view.stage_index > 0 {
        DrawMode::MagnifiedPreview
    } else {
        DrawMode::TransparentGrid
    }
}

fn format_jump_indicator(view: &JumpOverlayView) -> String {
    if let Some(adjust) = &view.final_adjust {
        if !adjust.show_hint {
            return "Adjust".to_string();
        }
        return format!(
            "Adjust: arrows move, {}+arrows {}px, {} ok, {} cancel, {} back",
            adjust.modifier_key,
            adjust.large_step_px,
            adjust.confirm_key,
            adjust.cancel_key,
            adjust.back_key
        );
    }

    format!(
        "Jump {}/{}: {}_",
        view.stage_index + 1,
        view.stage_count,
        view.input
    )
}

fn preview_source_rect(view: &JumpOverlayView, snapshot: &ScreenSnapshot) -> RECT {
    let preview_source_region = preview_source_region_for_view(view);
    let target_region = view.target_region;
    let behavior = active_stage_metadata(view)
        .map(|stage| stage.preview_edge_behavior)
        .unwrap_or_default();

    let region = match behavior {
        PreviewEdgeBehavior::Clamp | PreviewEdgeBehavior::AllowAsymmetricContext => {
            clamp_region_to_snapshot(preview_source_region, snapshot)
        }
        PreviewEdgeBehavior::ShiftIntoBounds => {
            shift_region_into_snapshot(preview_source_region, snapshot)
        }
        PreviewEdgeBehavior::DisableContextNearEdges => {
            if active_stage_metadata(view).is_some_and(|stage| {
                !context_would_exceed_bounds(
                    view.target_region,
                    stage.visual_context_margin_percent,
                    snapshot,
                )
            }) {
                preview_source_region
            } else {
                clamp_region_to_snapshot(target_region, snapshot)
            }
        }
    };

    region_to_rect(region)
}

fn context_would_exceed_bounds(
    region: JumpRegion,
    margin_percent: u8,
    snapshot: &ScreenSnapshot,
) -> bool {
    let margin_x = region.width * margin_percent as i32 / 100;
    let margin_y = region.height * margin_percent as i32 / 100;
    region.left - margin_x < snapshot.left
        || region.top - margin_y < snapshot.top
        || region.left + region.width + margin_x > snapshot.left + snapshot.width
        || region.top + region.height + margin_y > snapshot.top + snapshot.height
}

fn clamp_region_to_snapshot(region: JumpRegion, snapshot: &ScreenSnapshot) -> JumpRegion {
    let left = region.left.max(snapshot.left);
    let top = region.top.max(snapshot.top);
    let right = (region.left + region.width).min(snapshot.left + snapshot.width);
    let bottom = (region.top + region.height).min(snapshot.top + snapshot.height);
    JumpRegion {
        left,
        top,
        width: (right - left).max(0),
        height: (bottom - top).max(0),
    }
}

fn shift_region_into_snapshot(region: JumpRegion, snapshot: &ScreenSnapshot) -> JumpRegion {
    let width = region.width.min(snapshot.width).max(0);
    let height = region.height.min(snapshot.height).max(0);
    let min_left = snapshot.left;
    let max_left = snapshot.left + snapshot.width - width;
    let min_top = snapshot.top;
    let max_top = snapshot.top + snapshot.height - height;
    JumpRegion {
        left: region.left.clamp(min_left, max_left),
        top: region.top.clamp(min_top, max_top),
        width,
        height,
    }
}

fn map_axis(value: i32, source_start: i32, source_len: i32, dest_start: i32, dest_len: i32) -> i32 {
    (dest_start as f64 + (value - source_start) as f64 * dest_len as f64 / source_len as f64)
        .round() as i32
}

fn source_screen_to_client(
    x: i32,
    y: i32,
    source_region: JumpRegion,
    client_draw_region: JumpRegion,
) -> Option<(i32, i32)> {
    if !source_region.is_valid() || !client_draw_region.is_valid() {
        return None;
    }

    Some((
        map_axis(
            x,
            source_region.left,
            source_region.width,
            client_draw_region.left,
            client_draw_region.width,
        ),
        map_axis(
            y,
            source_region.top,
            source_region.height,
            client_draw_region.top,
            client_draw_region.height,
        ),
    ))
}

fn target_region_client_rect(
    target_region: JumpRegion,
    source_region: JumpRegion,
    client_draw_region: JumpRegion,
) -> Option<RECT> {
    project_source_rect_to_client(target_region, source_region, client_draw_region)
}

fn project_source_rect_to_client(
    source_rect: JumpRegion,
    source_region: JumpRegion,
    client_draw_region: JumpRegion,
) -> Option<RECT> {
    if !source_rect.is_valid() {
        return None;
    }
    let (left, top) = source_screen_to_client(
        source_rect.left,
        source_rect.top,
        source_region,
        client_draw_region,
    )?;
    let (right, bottom) = source_screen_to_client(
        source_rect.left + source_rect.width,
        source_rect.top + source_rect.height,
        source_region,
        client_draw_region,
    )?;

    Some(RECT {
        left,
        top,
        right,
        bottom,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JumpRenderBranches {
    target_outline: bool,
    preview_outline: bool,
    active_grid_outline: bool,
    cell_centers: bool,
    final_crosshair: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LabelRenderPlan {
    labels: bool,
    center_markers: bool,
    separators: bool,
}

fn label_render_plan(labels: JumpLabelMetadata, cell_w: i32, cell_h: i32) -> LabelRenderPlan {
    let large_enough = labels.hide_threshold_px <= 0
        || (cell_w >= labels.hide_threshold_px && cell_h >= labels.hide_threshold_px);
    LabelRenderPlan {
        labels: large_enough,
        center_markers: large_enough && labels.center_marker,
        separators: labels.separators,
    }
}

fn render_branches_for_view(view: &JumpOverlayView) -> JumpRenderBranches {
    JumpRenderBranches {
        target_outline: view.visuals.selected_region_outline,
        preview_outline: view.visuals.preview_outline,
        active_grid_outline: view.visuals.active_grid_outline,
        cell_centers: view.visuals.cell_centers,
        final_crosshair: view.visuals.final_crosshair && view.final_adjust.is_some(),
    }
}

fn current_cursor_position() -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        Some((point.x, point.y))
    }
}

fn bounded_region_centered_on(
    center: (i32, i32),
    size_source: JumpRegion,
    bounds: JumpRegion,
) -> Option<JumpRegion> {
    if !size_source.is_valid() || !bounds.is_valid() {
        return None;
    }

    let width = size_source.width.min(bounds.width);
    let height = size_source.height.min(bounds.height);
    let bounds_right = bounds.left + bounds.width;
    let bounds_bottom = bounds.top + bounds.height;
    let mut left = center.0 - width / 2;
    let mut top = center.1 - height / 2;
    left = left.max(bounds.left).min(bounds_right - width);
    top = top.max(bounds.top).min(bounds_bottom - height);

    Some(JumpRegion {
        left,
        top,
        width,
        height,
    })
}

fn zoomed_region_around_target(
    base_region: JumpRegion,
    target_region: JumpRegion,
    bounds: JumpRegion,
    zoom_scale: f32,
) -> Option<JumpRegion> {
    if !base_region.is_valid() || !target_region.is_valid() || !bounds.is_valid() {
        return None;
    }

    let zoom_scale = zoom_scale.max(1.0);
    let width = ((base_region.width as f32 / zoom_scale).round() as i32).max(1);
    let height = ((base_region.height as f32 / zoom_scale).round() as i32).max(1);
    let target_center = (
        target_region.left + target_region.width / 2,
        target_region.top + target_region.height / 2,
    );

    bounded_region_centered_on(
        target_center,
        JumpRegion {
            left: base_region.left,
            top: base_region.top,
            width,
            height,
        },
        bounds,
    )
}

fn active_stage_metadata(view: &JumpOverlayView) -> Option<&JumpStageMetadata> {
    view.stages.get(view.stage_index)
}

fn preview_source_region_for_view(view: &JumpOverlayView) -> JumpRegion {
    active_stage_metadata(view)
        .and_then(|stage| {
            let base_region = expand_region_within(
                view.target_region,
                stage.visual_context_margin_percent,
                view.session_region,
            )?;
            zoomed_region_around_target(
                base_region,
                view.target_region,
                view.session_region,
                stage.zoom_scale,
            )
        })
        .unwrap_or(view.preview_source_region)
}

fn target_region_for_stage_metadata(
    target_region: JumpRegion,
    session_region: JumpRegion,
    stage: &JumpStageMetadata,
) -> JumpRegion {
    match stage.target_region_mode {
        JumpTargetRegionMode::ExactRegion => target_region,
        JumpTargetRegionMode::RegionWithContext => {
            expand_region_within(target_region, stage.target_margin_percent, session_region)
                .unwrap_or(target_region)
        }
        JumpTargetRegionMode::ExpandedTarget => {
            expand_region_within(target_region, stage.target_margin_percent, session_region)
                .unwrap_or(target_region)
        }
        JumpTargetRegionMode::CursorCenteredZoom => current_cursor_position()
            .and_then(|cursor| bounded_region_centered_on(cursor, target_region, session_region))
            .unwrap_or(target_region),
    }
}

fn grid_target_region(view: &JumpOverlayView) -> JumpRegion {
    active_stage_metadata(view)
        .map(|stage| {
            target_region_for_stage_metadata(view.target_region, view.session_region, stage)
        })
        .unwrap_or(view.target_region)
}

pub struct JumpOverlay {
    hwnd: Option<HWND>,
    visible: bool,
    view: Option<JumpOverlayView>,
    snapshot: Option<ScreenSnapshot>,
    repaint_requested: bool,
}

impl JumpOverlay {
    pub fn new() -> Self {
        Self {
            hwnd: None,
            visible: false,
            view: None,
            snapshot: None,
            repaint_requested: false,
        }
    }

    fn request_repaint(&mut self) {
        self.repaint_requested = true;
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = InvalidateRect(Some(h), None, false);
            }
        }
    }

    fn create_window(&mut self) {
        if self.hwnd.is_some() {
            return;
        }
        unsafe {
            let h_instance = GetModuleHandleW(None).unwrap();
            let class = w!("JumpOverlayClass");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(jump_window_proc),
                hInstance: h_instance.into(),
                lpszClassName: class,
                style: CS_HREDRAW | CS_VREDRAW,
                hbrBackground: HBRUSH(ptr::null_mut()),
                ..Default::default()
            };
            let atom = RegisterClassW(&wc);
            if atom == 0 {
                println!("RegisterClassW failed: {:?}", GetLastError());
            }
            let screen_rect = ScreenRect::from_virtual_screen();
            let hwnd = CreateWindowExW(
                overlay_ex_style(),
                class,
                w!("JumpOverlay"),
                WS_POPUP,
                screen_rect.left,
                screen_rect.top,
                screen_rect.width,
                screen_rect.height,
                None,
                None,
                Some(h_instance.into()),
                None,
            );
            match hwnd {
                Ok(h) => {
                    apply_layered_attributes(h, transparency_mode());
                    let _ = ShowWindow(h, SW_HIDE);
                    self.hwnd = Some(h);
                }
                Err(e) => {
                    println!("CreateWindowExW failed: {:?}", e);
                }
            }
        }
    }

    pub fn initialize(&mut self, view: JumpOverlayView) {
        self.create_window();
        self.snapshot = capture_virtual_screen();
        self.view = Some(view);
        self.apply_view_layering();
        self.repaint_requested = false;
    }

    pub fn show(&mut self) {
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            }
            self.request_repaint();
            self.visible = true;
        }
    }

    pub fn hide(&mut self) {
        self.view = None;
        self.snapshot = None;
        if let Some(h) = self.hwnd {
            unsafe {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
        self.visible = false;
    }

    fn effective_draw_mode(&self) -> DrawMode {
        let Some(view) = &self.view else {
            return DrawMode::TransparentGrid;
        };
        match (draw_mode_for_view(view), self.snapshot.as_ref()) {
            (DrawMode::MagnifiedPreview, Some(_)) => DrawMode::MagnifiedPreview,
            _ => DrawMode::TransparentGrid,
        }
    }

    fn apply_view_layering(&self) {
        if let Some(hwnd) = self.hwnd {
            let mode = match self.effective_draw_mode() {
                DrawMode::TransparentGrid => transparency_mode(),
                DrawMode::MagnifiedPreview => TransparencyMode::Opaque {
                    alpha: OVERLAY_ALPHA,
                },
            };
            apply_layered_attributes(hwnd, mode);
        }
    }

    fn client_rect(&self) -> Option<RECT> {
        let hwnd = self.hwnd?;
        unsafe {
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).ok()?;
            Some(rect)
        }
    }

    fn client_draw_rect(&self, view: &JumpOverlayView, client_rect: RECT) -> JumpRegion {
        if view.client_draw_region.is_valid() {
            view.client_draw_region
        } else {
            JumpRegion {
                left: client_rect.left,
                top: client_rect.top,
                width: client_rect.right - client_rect.left,
                height: client_rect.bottom - client_rect.top,
            }
        }
    }

    fn grid_rect(&self, view: &JumpOverlayView, client_rect: RECT) -> Option<RECT> {
        target_region_client_rect(
            grid_target_region(view),
            preview_source_region_for_view(view),
            self.client_draw_rect(view, client_rect),
        )
    }

    fn draw_rect_outline(&self, hdc: HDC, rect: RECT, color: COLORREF, pen_width: i32) {
        unsafe {
            let pen = CreatePen(PS_SOLID, pen_width, color);
            let old_pen = SelectObject(hdc, pen.into());
            let _ = MoveToEx(hdc, rect.left, rect.top, None);
            let _ = LineTo(hdc, rect.right, rect.top);
            let _ = LineTo(hdc, rect.right, rect.bottom);
            let _ = LineTo(hdc, rect.left, rect.bottom);
            let _ = LineTo(hdc, rect.left, rect.top);
            let _ = SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen.into());
        }
    }

    fn draw_target_outline(
        &self,
        hdc: HDC,
        view: &JumpOverlayView,
        client_draw_region: JumpRegion,
    ) {
        let Some(rect) = target_region_client_rect(
            view.target_region,
            preview_source_region_for_view(view),
            client_draw_region,
        ) else {
            return;
        };

        self.draw_rect_outline(hdc, rect, target_outline_color(), 2);
    }

    fn draw_preview_outline(&self, hdc: HDC, view: &JumpOverlayView, client_rect: RECT) {
        self.draw_rect_outline(
            hdc,
            region_to_rect(self.client_draw_rect(view, client_rect)),
            preview_outline_color(),
            1,
        );
    }

    fn draw_active_grid_outline(&self, hdc: HDC, grid_rect: RECT) {
        self.draw_rect_outline(hdc, grid_rect, active_grid_outline_color(), 2);
    }

    fn draw_cell_centers(
        &self,
        hdc: HDC,
        grid_rect: RECT,
        grid_size: (u32, u32),
        cell_w: i32,
        cell_h: i32,
    ) {
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, cell_center_color());
            let old_pen = SelectObject(hdc, pen.into());
            for row in 0..grid_size.1 {
                for col in 0..grid_size.0 {
                    let x = grid_rect.left + col as i32 * cell_w + cell_w / 2;
                    let y = grid_rect.top + row as i32 * cell_h + cell_h / 2;
                    let _ = MoveToEx(hdc, x - 2, y, None);
                    let _ = LineTo(hdc, x + 3, y);
                    let _ = MoveToEx(hdc, x, y - 2, None);
                    let _ = LineTo(hdc, x, y + 3);
                }
            }
            let _ = SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen.into());
        }
    }

    fn draw_final_crosshair(&self, hdc: HDC, view: &JumpOverlayView) {
        let Some(adjust) = &view.final_adjust else {
            return;
        };
        let Some((x, y)) = source_screen_to_client(
            adjust.candidate_point.0,
            adjust.candidate_point.1,
            preview_source_region_for_view(view),
            view.client_draw_region,
        ) else {
            return;
        };

        unsafe {
            let pen = CreatePen(PS_SOLID, 2, final_adjust_color());
            let old_pen = SelectObject(hdc, pen.into());
            let _ = MoveToEx(hdc, x - 12, y, None);
            let _ = LineTo(hdc, x + 13, y);
            let _ = MoveToEx(hdc, x, y - 12, None);
            let _ = LineTo(hdc, x, y + 13);
            let _ = SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen.into());
        }
    }

    fn draw_background(&self, hdc: HDC, client_rect: &RECT, view: &JumpOverlayView) {
        unsafe {
            match self.effective_draw_mode() {
                DrawMode::TransparentGrid => {
                    let bg_brush = CreateSolidBrush(overlay_colorkey());
                    let _ = FillRect(hdc, client_rect, bg_brush);
                    let _ = DeleteObject(bg_brush.into());
                }
                DrawMode::MagnifiedPreview => {
                    if let Some(snapshot) = &self.snapshot {
                        let _ = BitBlt(
                            hdc,
                            0,
                            0,
                            snapshot.width,
                            snapshot.height,
                            Some(snapshot.hdc()),
                            0,
                            0,
                            SRCCOPY,
                        );

                        let src = preview_source_rect(view, snapshot);
                        let src_w = src.right - src.left;
                        let src_h = src.bottom - src.top;
                        if src_w > 0 && src_h > 0 {
                            let draw_region = self.client_draw_rect(view, *client_rect);
                            let _ = SetStretchBltMode(hdc, HALFTONE);
                            let _ = StretchBlt(
                                hdc,
                                draw_region.left,
                                draw_region.top,
                                draw_region.width,
                                draw_region.height,
                                Some(snapshot.hdc()),
                                snapshot.source_x(src.left),
                                snapshot.source_y(src.top),
                                src_w,
                                src_h,
                                SRCCOPY,
                            );
                        }
                    }
                }
            }
        }
    }

    fn draw(&self, hdc: HDC) {
        if self.hwnd.is_some() {
            let Some(view) = &self.view else {
                return;
            };
            let grid_size = view.grid_size;
            if grid_size.0 == 0 || grid_size.1 == 0 {
                return;
            }
            unsafe {
                let Some(rect) = self.client_rect() else {
                    return;
                };
                self.draw_background(hdc, &rect, view);
                let client_draw_region = self.client_draw_rect(view, rect);
                let branches = render_branches_for_view(view);
                if branches.preview_outline {
                    self.draw_preview_outline(hdc, view, rect);
                }
                if branches.target_outline {
                    self.draw_target_outline(hdc, view, client_draw_region);
                }

                let Some(grid_rect) = self.grid_rect(view, rect) else {
                    return;
                };
                let width = grid_rect.right - grid_rect.left;
                let height = grid_rect.bottom - grid_rect.top;
                let cell_w = width / grid_size.0 as i32;
                let cell_h = height / grid_size.1 as i32;
                if cell_w <= 0 || cell_h <= 0 {
                    return;
                }
                let labels = active_stage_metadata(view)
                    .map(|stage| stage.labels)
                    .unwrap_or(crate::JumpLabelConfig::default().into());
                let label_plan = label_render_plan(labels, cell_w, cell_h);

                let old_bk_mode = SetBkMode(hdc, TRANSPARENT);
                let old_text_color = SetTextColor(hdc, label_color());

                let pen = CreatePen(PS_SOLID, 1, grid_color());
                let old_pen = SelectObject(hdc, pen.into());

                if label_plan.separators {
                    for x in 0..=grid_size.0 {
                        let pos = grid_rect.left + (x as i32 * cell_w);
                        let _ = MoveToEx(hdc, pos, grid_rect.top, None);
                        let _ = LineTo(hdc, pos, grid_rect.bottom);
                    }
                    for y in 0..=grid_size.1 {
                        let pos = grid_rect.top + (y as i32 * cell_h);
                        let _ = MoveToEx(hdc, grid_rect.left, pos, None);
                        let _ = LineTo(hdc, grid_rect.right, pos);
                    }
                }
                if branches.active_grid_outline {
                    self.draw_active_grid_outline(hdc, grid_rect);
                }
                if branches.cell_centers || label_plan.center_markers {
                    self.draw_cell_centers(hdc, grid_rect, grid_size, cell_w, cell_h);
                }

                if label_plan.labels {
                    let row_len = letters_needed(grid_size.1);
                    let col_len = letters_needed(grid_size.0);
                    for row in 0..grid_size.1 {
                        let row_code = index_to_code(row as usize, row_len);
                        for col in 0..grid_size.0 {
                            let col_code = index_to_code(col as usize, col_len);
                            let code = format!("{}{}", row_code, col_code);
                            let text: Vec<u16> = code.encode_utf16().collect();
                            let offset = (8.0 * labels.font_scale).round() as i32;
                            let x = grid_rect.left + col as i32 * cell_w + cell_w / 2 - offset;
                            let y = grid_rect.top + row as i32 * cell_h + cell_h / 2 - offset;
                            let _ = TextOutW(hdc, x, y, &text);
                        }
                    }
                }

                let _ = SetTextColor(hdc, input_color());
                let indicator = format_jump_indicator(view);
                let indicator_utf16: Vec<u16> = indicator.encode_utf16().collect();
                let indicator_x = grid_rect.left + (width / 2) - 60;
                let indicator_y = grid_rect.top + 16;
                let _ = TextOutW(hdc, indicator_x, indicator_y, &indicator_utf16);
                if branches.final_crosshair {
                    self.draw_final_crosshair(hdc, view);
                }

                let _ = SetTextColor(hdc, old_text_color);
                let _ = SetBkMode(hdc, BACKGROUND_MODE(old_bk_mode as u32));
                let _ = SelectObject(hdc, old_pen);
                let _ = DeleteObject(pen.into());
            }
        }
    }

    pub fn update_view(&mut self, view: JumpOverlayView) {
        self.view = Some(view);
        self.apply_view_layering();
        self.request_repaint();
    }
}

impl Drop for JumpOverlay {
    fn drop(&mut self) {
        self.snapshot = None;
    }
}

impl From<ScreenRect> for JumpRegion {
    fn from(rect: ScreenRect) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        }
    }
}

extern "system" fn jump_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let ps = &mut PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, ps) };
            JUMP_OVERLAY.with(|overlay| {
                let mut ov = overlay.borrow_mut();
                ov.draw(hdc);
                ov.repaint_requested = false;
            });
            unsafe {
                let _ = EndPaint(hwnd, ps);
            }
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn show_jump_overlay(view: JumpOverlayView) {
    JUMP_OVERLAY.with(|overlay| {
        let mut ov = overlay.borrow_mut();
        ov.initialize(view);
        ov.show();
    });
}

pub fn update_jump_overlay(view: JumpOverlayView) {
    JUMP_OVERLAY.with(|overlay| overlay.borrow_mut().update_view(view));
}

pub fn hide_jump_overlay() {
    JUMP_OVERLAY.with(|overlay| overlay.borrow_mut().hide());
}

#[cfg(test)]
mod tests {
    use super::{
        bounded_region_centered_on, draw_mode_for_view, format_jump_indicator, grid_target_region,
        label_render_plan, overlay_colorkey, overlay_ex_style, preview_source_rect,
        preview_source_region_for_view, project_source_rect_to_client, render_branches_for_view,
        source_screen_to_client, target_region_client_rect, transparency_mode, DrawMode,
        JumpRenderBranches, TransparencyMode, OVERLAY_ALPHA,
    };
    use crate::jump_session::JumpRegion;
    use crate::jump_view::{
        FinalAdjustOverlayView, JumpLabelMetadata, JumpOverlayView, JumpStageMetadata, JumpVisuals,
    };
    use crate::screen_capture::ScreenSnapshot;
    use crate::{JumpAimPoint, JumpLabelConfig, JumpTargetRegionMode, PreviewEdgeBehavior};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{WS_EX_NOACTIVATE, WS_EX_TRANSPARENT};

    #[test]
    fn style_contains_non_activate_and_click_through_bits() {
        let style = overlay_ex_style();
        assert!(style.contains(WS_EX_NOACTIVATE));
        assert!(style.contains(WS_EX_TRANSPARENT));
    }

    #[test]
    fn jump_indicator_formatting() {
        assert_eq!(format_jump_indicator(&view(0, 1, "")), "Jump 1/1: _");
        assert_eq!(format_jump_indicator(&view(1, 3, "AB")), "Jump 2/3: AB_");
    }

    #[test]
    fn final_adjust_hint_respects_visibility_toggle() {
        let mut view = view(0, 1, "");
        view.final_adjust = Some(FinalAdjustOverlayView {
            original_point: (0, 0),
            candidate_point: (1, 1),
            region: view.target_region,
            small_step_px: 1,
            large_step_px: 10,
            modifier_key: "Shift".to_string(),
            confirm_key: "Enter".to_string(),
            cancel_key: "Escape".to_string(),
            back_key: "Backspace".to_string(),
            show_hint: false,
        });

        assert_eq!(format_jump_indicator(&view), "Adjust");
    }

    #[test]
    fn transparency_mode_is_colorkey_black() {
        assert_eq!(
            transparency_mode(),
            TransparencyMode::ColorKey {
                color: overlay_colorkey(),
                alpha: OVERLAY_ALPHA
            }
        );
    }

    #[test]
    fn first_stage_uses_transparent_grid() {
        assert_eq!(
            draw_mode_for_view(&view(0, 2, "")),
            DrawMode::TransparentGrid
        );
    }

    #[test]
    fn later_stages_use_magnified_preview() {
        assert_eq!(
            draw_mode_for_view(&view(1, 2, "")),
            DrawMode::MagnifiedPreview
        );
    }

    #[test]
    fn source_screen_to_client_maps_edges_with_negative_origin() {
        let source = JumpRegion {
            left: -100,
            top: 50,
            width: 200,
            height: 100,
        };
        let client = JumpRegion {
            left: 0,
            top: 0,
            width: 800,
            height: 400,
        };

        assert_eq!(
            source_screen_to_client(-100, 50, source, client),
            Some((0, 0))
        );
        assert_eq!(
            source_screen_to_client(100, 150, source, client),
            Some((800, 400))
        );
        assert_eq!(
            source_screen_to_client(0, 100, source, client),
            Some((400, 200))
        );
    }

    #[test]
    fn target_region_client_rect_maps_non_square_context() {
        let source = JumpRegion {
            left: 10,
            top: 20,
            width: 300,
            height: 100,
        };
        let target = JumpRegion {
            left: 85,
            top: 45,
            width: 150,
            height: 50,
        };
        let client = JumpRegion {
            left: 20,
            top: 30,
            width: 600,
            height: 400,
        };

        let rect = target_region_client_rect(target, source, client).unwrap();
        assert_eq!(rect.left, 170);
        assert_eq!(rect.top, 130);
        assert_eq!(rect.right, 470);
        assert_eq!(rect.bottom, 330);
    }

    #[test]
    fn projected_rect_alignment_uses_explicit_source_to_client_transform() {
        let preview_source = JumpRegion {
            left: 100,
            top: 200,
            width: 50,
            height: 25,
        };
        let target = JumpRegion {
            left: 110,
            top: 205,
            width: 20,
            height: 10,
        };
        let client = JumpRegion {
            left: 400,
            top: 20,
            width: 500,
            height: 250,
        };

        let rect = project_source_rect_to_client(target, preview_source, client).unwrap();

        assert_eq!(rect.left, 500);
        assert_eq!(rect.top, 70);
        assert_eq!(rect.right, 700);
        assert_eq!(rect.bottom, 170);
    }

    #[test]
    fn visual_toggles_control_render_branches() {
        let mut view = view(1, 2, "");
        view.final_adjust = Some(FinalAdjustOverlayView {
            original_point: (0, 0),
            candidate_point: (1, 1),
            region: view.target_region,
            small_step_px: 1,
            large_step_px: 10,
            modifier_key: "Shift".to_string(),
            confirm_key: "Enter".to_string(),
            cancel_key: "Escape".to_string(),
            back_key: "Backspace".to_string(),
            show_hint: true,
        });
        view.visuals = JumpVisuals {
            selected_region_outline: false,
            preview_outline: true,
            active_grid_outline: false,
            cell_centers: true,
            final_crosshair: false,
        };

        assert_eq!(
            render_branches_for_view(&view),
            JumpRenderBranches {
                target_outline: false,
                preview_outline: true,
                active_grid_outline: false,
                cell_centers: true,
                final_crosshair: false,
            }
        );
    }

    #[test]
    fn final_crosshair_branch_requires_final_adjust_view() {
        let mut view = view(1, 2, "");
        view.visuals.final_crosshair = true;

        assert!(!render_branches_for_view(&view).final_crosshair);
    }

    #[test]
    fn exact_region_grid_target_preserves_selected_bounds() {
        let mut view = view(1, 2, "");
        view.target_region = JumpRegion {
            left: 25,
            top: 35,
            width: 40,
            height: 20,
        };
        view.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            stage_metadata(1, JumpTargetRegionMode::ExactRegion),
        ];

        assert_eq!(grid_target_region(&view), view.target_region);
    }

    #[test]
    fn expanded_target_grid_target_uses_target_margin() {
        let mut view = view(1, 2, "");
        view.target_region = JumpRegion {
            left: 25,
            top: 35,
            width: 40,
            height: 20,
        };
        view.session_region = JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        };
        view.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            stage_metadata(1, JumpTargetRegionMode::ExpandedTarget),
        ];

        assert_eq!(
            grid_target_region(&view),
            JumpRegion {
                left: 21,
                top: 33,
                width: 48,
                height: 24,
            }
        );
    }

    #[test]
    fn region_with_context_grid_target_ignores_visual_context_margin() {
        let mut view = view(1, 2, "");
        view.target_region = JumpRegion {
            left: 25,
            top: 35,
            width: 40,
            height: 20,
        };
        view.session_region = JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        };
        view.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            JumpStageMetadata {
                index: 1,
                grid_size: (10, 10),
                aim_point: JumpAimPoint::Center,
                aim_offset_x_px: 0,
                aim_offset_y_px: 0,
                target_margin_percent: 0,
                visual_context_margin_percent: 20,
                zoom_scale: 1.0,
                target_region_mode: JumpTargetRegionMode::RegionWithContext,
                preview_edge_behavior: PreviewEdgeBehavior::Clamp,
                labels: JumpLabelConfig::default().into(),
            },
        ];

        assert_eq!(grid_target_region(&view), view.target_region);
    }

    #[test]
    fn preview_source_region_uses_visual_context_margin() {
        let mut view = view(1, 2, "");
        view.target_region = JumpRegion {
            left: 25,
            top: 35,
            width: 40,
            height: 20,
        };
        view.session_region = JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        };
        view.preview_source_region = view.target_region;
        view.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            JumpStageMetadata {
                index: 1,
                grid_size: (10, 10),
                aim_point: JumpAimPoint::Center,
                aim_offset_x_px: 0,
                aim_offset_y_px: 0,
                target_margin_percent: 0,
                visual_context_margin_percent: 20,
                zoom_scale: 1.0,
                target_region_mode: JumpTargetRegionMode::ExactRegion,
                preview_edge_behavior: PreviewEdgeBehavior::Clamp,
                labels: JumpLabelConfig::default().into(),
            },
        ];

        assert_eq!(
            preview_source_region_for_view(&view),
            JumpRegion {
                left: 17,
                top: 31,
                width: 56,
                height: 28,
            }
        );
    }

    #[test]
    fn preview_source_region_area_decreases_as_zoom_increases() {
        let mut low_zoom = view(1, 2, "");
        low_zoom.target_region = JumpRegion {
            left: 100,
            top: 100,
            width: 100,
            height: 80,
        };
        low_zoom.session_region = JumpRegion {
            left: 0,
            top: 0,
            width: 400,
            height: 400,
        };
        low_zoom.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            JumpStageMetadata {
                index: 1,
                grid_size: (10, 10),
                aim_point: JumpAimPoint::Center,
                aim_offset_x_px: 0,
                aim_offset_y_px: 0,
                target_margin_percent: 0,
                visual_context_margin_percent: 50,
                zoom_scale: 1.0,
                target_region_mode: JumpTargetRegionMode::ExactRegion,
                preview_edge_behavior: PreviewEdgeBehavior::Clamp,
                labels: JumpLabelConfig::default().into(),
            },
        ];
        let mut high_zoom = low_zoom.clone();
        high_zoom.stages[1].zoom_scale = 2.0;

        let low_region = preview_source_region_for_view(&low_zoom);
        let high_region = preview_source_region_for_view(&high_zoom);
        let low_area = low_region.width * low_region.height;
        let high_area = high_region.width * high_region.height;

        assert!(high_area < low_area);
        assert_eq!(
            low_region,
            JumpRegion {
                left: 50,
                top: 60,
                width: 200,
                height: 160,
            }
        );
        assert_eq!(
            high_region,
            JumpRegion {
                left: 100,
                top: 100,
                width: 100,
                height: 80,
            }
        );
    }

    #[test]
    fn preview_edge_behavior_shifts_source_into_snapshot() {
        let mut view = view(1, 2, "");
        view.target_region = JumpRegion {
            left: 5,
            top: 5,
            width: 20,
            height: 20,
        };
        view.session_region = JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        };
        view.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            JumpStageMetadata {
                preview_edge_behavior: PreviewEdgeBehavior::ShiftIntoBounds,
                visual_context_margin_percent: 50,
                ..stage_metadata(1, JumpTargetRegionMode::ExactRegion)
            },
        ];
        let snapshot = ScreenSnapshot::test_bounds(0, 0, 100, 100);

        assert_eq!(
            preview_source_rect(&view, &snapshot),
            RECT {
                left: 0,
                top: 0,
                right: 35,
                bottom: 35
            }
        );
    }

    #[test]
    fn preview_edge_behavior_disables_context_near_edges() {
        let mut view = view(1, 2, "");
        view.target_region = JumpRegion {
            left: 5,
            top: 5,
            width: 20,
            height: 20,
        };
        view.session_region = JumpRegion {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        };
        view.stages = vec![
            stage_metadata(0, JumpTargetRegionMode::ExactRegion),
            JumpStageMetadata {
                preview_edge_behavior: PreviewEdgeBehavior::DisableContextNearEdges,
                visual_context_margin_percent: 50,
                ..stage_metadata(1, JumpTargetRegionMode::ExactRegion)
            },
        ];
        let snapshot = ScreenSnapshot::test_bounds(0, 0, 100, 100);

        assert_eq!(
            preview_source_rect(&view, &snapshot),
            RECT {
                left: 5,
                top: 5,
                right: 25,
                bottom: 25
            }
        );
    }

    #[test]
    fn label_plan_hides_labels_below_threshold_but_keeps_separators_configurable() {
        let labels = JumpLabelMetadata {
            font_scale: 1.0,
            center_marker: true,
            separators: false,
            hide_threshold_px: 20,
        };

        assert_eq!(
            label_render_plan(labels, 10, 30),
            super::LabelRenderPlan {
                labels: false,
                center_markers: false,
                separators: false
            }
        );
        assert_eq!(
            label_render_plan(labels, 20, 30),
            super::LabelRenderPlan {
                labels: true,
                center_markers: true,
                separators: false
            }
        );
    }

    #[test]
    fn centered_region_clamps_to_bounds() {
        assert_eq!(
            bounded_region_centered_on(
                (95, 95),
                JumpRegion {
                    left: 0,
                    top: 0,
                    width: 30,
                    height: 20,
                },
                JumpRegion {
                    left: 0,
                    top: 0,
                    width: 100,
                    height: 100,
                },
            ),
            Some(JumpRegion {
                left: 70,
                top: 80,
                width: 30,
                height: 20,
            })
        );
    }

    fn stage_metadata(index: usize, target_region_mode: JumpTargetRegionMode) -> JumpStageMetadata {
        JumpStageMetadata {
            index,
            grid_size: (10, 10),
            aim_point: JumpAimPoint::Center,
            aim_offset_x_px: 0,
            aim_offset_y_px: 0,
            target_margin_percent: 10,
            visual_context_margin_percent: 20,
            zoom_scale: 1.0,
            target_region_mode,
            preview_edge_behavior: PreviewEdgeBehavior::Clamp,
            labels: JumpLabelConfig::default().into(),
        }
    }

    fn view(stage_index: usize, stage_count: usize, input: &str) -> JumpOverlayView {
        JumpOverlayView {
            stage_index,
            stage_count,
            stages: vec![stage_metadata(0, JumpTargetRegionMode::ExactRegion)],
            session_region: JumpRegion {
                left: -100,
                top: 50,
                width: 200,
                height: 100,
            },
            target_region: JumpRegion {
                left: -100,
                top: 50,
                width: 200,
                height: 100,
            },
            preview_source_region: JumpRegion {
                left: -110,
                top: 45,
                width: 220,
                height: 110,
            },
            client_draw_region: JumpRegion {
                left: 0,
                top: 0,
                width: 800,
                height: 400,
            },
            grid_size: (10, 10),
            input: input.to_string(),
            visuals: JumpVisuals {
                selected_region_outline: true,
                preview_outline: true,
                active_grid_outline: true,
                cell_centers: false,
                final_crosshair: true,
            },
            final_adjust: None,
        }
    }
}
