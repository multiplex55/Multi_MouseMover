use crate::jump_session::JumpRegion;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpStageMetadata {
    pub index: usize,
    pub grid_size: (u32, u32),
    pub preview_margin_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpOverlayView {
    pub stage_index: usize,
    pub stage_count: usize,
    pub stages: Vec<JumpStageMetadata>,
    pub region: JumpRegion,
    pub grid_size: (u32, u32),
    pub input: String,
    pub preview_margin_percent: u8,
}
