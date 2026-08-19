use crate::blueprint::viewport_command::{BlueprintCommand, BlueprintCommandQueue};
use crate::blueprint::{PaneBlueprint, PaneKind};
use egui::Widget;

pub struct AddViewModal {
    pub is_open: bool,
    pub target_container: Option<crate::blueprint::ids::ContainerId>,
}

impl AddViewModal {
    pub fn new() -> Self {
        Self { is_open: false, target_container: None }
    }

    pub fn show(&mut self, ctx: &egui::Context, command_queue: &BlueprintCommandQueue) {
        if !self.is_open {
            return;
        }

        let title = if let Some(id) = self.target_container {
            format!("Add to Container {:?}", id)
        } else {
            "Add View or Container".to_string()
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label("Views");
                    ui.horizontal_wrapped(|ui| {
                        for (_icon, label, kind) in [
                            (&crate::icons::VIEW_TIMESERIES, "Trace", PaneKind::TraceView),
                            (&crate::icons::VIEW_GENERIC, "Stacked", PaneKind::StackedView),
                            (&crate::icons::VIEW_GENERIC, "Graph", PaneKind::NodeGraph),
                            (&crate::icons::VIEW_GENERIC, "Raster", PaneKind::RasterPlot),
                            (&crate::icons::VIEW_GENERIC, "PCA", PaneKind::PcaView),
                            (&crate::icons::VIEW_GENERIC, "Waveform Overlay", PaneKind::WaveformOverlay),
                            (&crate::icons::VIEW_GENERIC, "Multichannel", PaneKind::TimeseriesMultichannel),
                            (&crate::icons::VIEW_GENERIC, "Sorting", PaneKind::SpikeSorting),
                        ] {
                            if ui.button(label).clicked() {
                                command_queue.push(BlueprintCommand::AddPane {
                                    pane: PaneBlueprint::new(kind),
                                    parent_container: self.target_container,
                                    position_in_parent: None,
                                });
                                self.is_open = false;
                            }
                        }
                    });

                    ui.separator();
                    ui.label("Containers");
                    ui.horizontal(|ui| {
                        for (icon, label, kind) in [
                            (&crate::icons::CONTAINER_TABS, "Tabs", egui_tiles::ContainerKind::Tabs),
                            (&crate::icons::CONTAINER_HORIZONTAL, "Horizontal", egui_tiles::ContainerKind::Horizontal),
                            (&crate::icons::CONTAINER_VERTICAL, "Vertical", egui_tiles::ContainerKind::Vertical),
                            (&crate::icons::CONTAINER_GRID, "Grid", egui_tiles::ContainerKind::Grid),
                        ] {
                            if icon.as_button().ui(ui).on_hover_text(label).clicked() {
                                command_queue.push(BlueprintCommand::AddContainer {
                                    container_kind: kind,
                                    parent_container: self.target_container,
                                });
                                self.is_open = false;
                            }
                        }
                    });

                    ui.separator();
                    if ui.button("Cancel").clicked() {
                        self.is_open = false;
                    }
                });
            });
    }
}
