use crate::{jump_session::JumpRegion, JumpAimPoint, JumpTargetRegionMode, PreviewEdgeBehavior};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpVisuals {
    pub selected_region_outline: bool,
    pub preview_outline: bool,
    pub active_grid_outline: bool,
    pub cell_centers: bool,
    pub final_crosshair: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JumpStageMetadata {
    pub index: usize,
    pub grid_size: (u32, u32),
    pub aim_point: JumpAimPoint,
    pub aim_offset_x_px: i32,
    pub aim_offset_y_px: i32,
    pub target_margin_percent: u8,
    pub visual_context_margin_percent: u8,
    pub zoom_scale: f32,
    pub target_region_mode: JumpTargetRegionMode,
    pub preview_edge_behavior: PreviewEdgeBehavior,
    pub labels: JumpLabelMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpLabelMetadata {
    pub font_scale: f32,
    pub center_marker: bool,
    pub separators: bool,
    pub hide_threshold_px: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinalAdjustOverlayView {
    pub original_point: (i32, i32),
    pub candidate_point: (i32, i32),
    pub region: JumpRegion,
    pub small_step_px: i32,
    pub large_step_px: i32,
    pub modifier_key: String,
    pub confirm_key: String,
    pub cancel_key: String,
    pub back_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JumpOverlayView {
    pub stage_index: usize,
    pub stage_count: usize,
    pub stages: Vec<JumpStageMetadata>,
    pub session_region: JumpRegion,
    pub target_region: JumpRegion,
    pub preview_source_region: JumpRegion,
    pub client_draw_region: JumpRegion,
    pub grid_size: (u32, u32),
    pub input: String,
    pub visuals: JumpVisuals,
    pub final_adjust: Option<FinalAdjustOverlayView>,
}
