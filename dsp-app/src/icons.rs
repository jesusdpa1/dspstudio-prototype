use egui::{Image, ImageSource};

#[derive(Clone, Copy, Debug)]
pub struct Icon {
    pub uri: &'static str,
    pub image_bytes: &'static [u8],
}

impl Icon {
    #[inline]
    pub const fn new(uri: &'static str, image_bytes: &'static [u8]) -> Self {
        Self { uri, image_bytes }
    }

    #[inline]
    pub fn as_image_source(&self) -> ImageSource<'static> {
        ImageSource::Bytes {
            uri: self.uri.into(),
            bytes: self.image_bytes.into(),
        }
    }

    #[inline]
    pub fn as_image(&self) -> Image<'static> {
        Image::new(self.as_image_source())
    }

    #[inline]
    pub fn as_button(&self) -> egui::Button<'_> {
        egui::Button::image(self.as_image()).image_tint_follows_text_color(true)
    }
}

macro_rules! icon_from_path {
    ($path:literal) => {
        Icon::new(
            concat!("bytes://", $path),
            include_bytes!(concat!("../assets/icons/", $path))
        )
    };
}

// Global UI
pub const SEARCH: Icon = icon_from_path!("search.svg");
pub const ADD: Icon = icon_from_path!("add.svg");
pub const REMOVE: Icon = icon_from_path!("remove.svg");
pub const CLOSE: Icon = icon_from_path!("close.svg");
pub const MORE: Icon = icon_from_path!("more.svg");
pub const VISIBLE: Icon = icon_from_path!("visible.svg");
pub const INVISIBLE: Icon = icon_from_path!("invisible.svg");

// Panels
pub const RIGHT_PANEL_TOGGLE: Icon = icon_from_path!("right_panel_toggle.svg");
pub const LEFT_PANEL_TOGGLE: Icon = icon_from_path!("left_panel_toggle.svg");
pub const BOTTOM_PANEL_TOGGLE: Icon = icon_from_path!("bottom_panel_toggle.svg");

// Containers
pub const CONTAINER_HORIZONTAL: Icon = icon_from_path!("container_horizontal.svg");
pub const CONTAINER_VERTICAL: Icon = icon_from_path!("container_vertical.svg");
pub const CONTAINER_TABS: Icon = icon_from_path!("container_tabs.svg");
pub const CONTAINER_GRID: Icon = icon_from_path!("container_grid.svg");

// Views
pub const VIEW_GENERIC: Icon = icon_from_path!("view_generic.svg");
pub const VIEW_TIMESERIES: Icon = icon_from_path!("view_timeseries.svg");
pub const VIEW_2D: Icon = icon_from_path!("view_2d.svg");
pub const VIEW_3D: Icon = icon_from_path!("view_3d.svg");

// Data
pub const RECORDING: Icon = icon_from_path!("recording.svg");
pub const DATASET: Icon = icon_from_path!("dataset.svg");
pub const DATA_SOURCE: Icon = icon_from_path!("data_source.svg");
pub const BLUEPRINT: Icon = icon_from_path!("blueprint.svg");
pub const ENTITY: Icon = icon_from_path!("entity.svg");
pub const COMPONENT: Icon = icon_from_path!("component.svg");

// Drag and drop
pub const DND_HANDLE: Icon = icon_from_path!("dnd_handle.svg");
pub const DND_MOVE: Icon = icon_from_path!("dnd_move.svg");
