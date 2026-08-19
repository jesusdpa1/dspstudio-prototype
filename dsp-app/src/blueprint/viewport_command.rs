use std::sync::Arc;
use std::sync::Mutex;
use crate::blueprint::ids::{PaneId, ContainerId, Contents};
use crate::blueprint::pane::{PaneBlueprint, PaneConfig};

#[derive(Default, Clone)]
pub struct BlueprintCommandQueue {
    commands: Arc<Mutex<Vec<BlueprintCommand>>>,
}

impl BlueprintCommandQueue {
    pub fn push(&self, command: BlueprintCommand) {
        self.commands.lock().unwrap().push(command);
    }

    pub fn drain(&self) -> Vec<BlueprintCommand> {
        std::mem::take(&mut self.commands.lock().unwrap())
    }
}

#[derive(Debug, Clone)]
pub enum BlueprintCommand {
    SetTree(egui_tiles::Tree<PaneId>),
    AddPane {
        pane: PaneBlueprint,
        parent_container: Option<ContainerId>,
        position_in_parent: Option<usize>,
    },
    AddContainer {
        container_kind: egui_tiles::ContainerKind,
        parent_container: Option<ContainerId>,
    },
    SetContainerKind(ContainerId, egui_tiles::ContainerKind),
    FocusTab(PaneId),
    RemoveContents(Contents),
    SimplifyContainer(ContainerId, egui_tiles::SimplificationOptions),
    MakeAllChildrenSameSize(ContainerId),
    MoveContents {
        contents_to_move: Vec<Contents>,
        target_container: ContainerId,
        target_position_in_container: usize,
    },
    MoveContentsToNewContainer {
        contents_to_move: Vec<Contents>,
        new_container_kind: egui_tiles::ContainerKind,
        target_container: ContainerId,
        target_position_in_container: usize,
    },
    // dsp-app additions:
    RenamePane(PaneId, String),
    RenameContainer(ContainerId, String),
    SetPaneVisible(PaneId, bool),
    SetMaximized(Option<PaneId>),
    SetPaneConfig(PaneId, PaneConfig),
    SetContainerVisible(ContainerId, bool),
}
