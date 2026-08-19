use crate::blueprint::ids::{ContainerId, Contents};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContainerBlueprint {
    pub id: ContainerId,
    pub container_kind: egui_tiles::ContainerKind,
    pub display_name: Option<String>,
    pub contents: Vec<Contents>,
    #[serde(default)]
    pub col_shares: Vec<f32>,
    #[serde(default)]
    pub row_shares: Vec<f32>,
    pub active_tab: Option<Contents>,
    pub visible: bool,
    pub grid_columns: Option<u32>,
}

impl ContainerBlueprint {
    pub fn new(kind: egui_tiles::ContainerKind, contents: Vec<Contents>) -> Self {
        Self {
            id: ContainerId::new(),
            container_kind: kind,
            display_name: None,
            contents,
            col_shares: Vec::new(),
            row_shares: Vec::new(),
            active_tab: None,
            visible: true,
            grid_columns: None,
        }
    }
}
