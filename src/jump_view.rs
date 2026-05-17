use crate::{jump_session::JumpRegion, JumpTargetRegionMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpStageMetadata {
    pub index: usize,
    pub grid_size: (u32, u32),
    pub target_margin_percent: u8,
    pub visual_context_margin_percent: u8,
    pub target_region_mode: JumpTargetRegionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}
