use egui_tiles::TileId;
use uuid::Uuid;

/// Stable, serialization-safe pane identity backed by a random UUID.
///
/// The sequential AtomicU64 counter it replaced caused ID collisions when a
/// saved blueprint was loaded and new panes were added (counter restarts at 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         serde::Serialize, serde::Deserialize)]
pub struct PaneId(pub Uuid);

impl PaneId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PaneId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         serde::Serialize, serde::Deserialize)]
pub struct ContainerId(pub Uuid);

impl ContainerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ContainerId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Contents {
    Pane(PaneId),
    Container(ContainerId),
}

impl From<PaneId> for Contents {
    fn from(id: PaneId) -> Self {
        Self::Pane(id)
    }
}

impl From<ContainerId> for Contents {
    fn from(id: ContainerId) -> Self {
        Self::Container(id)
    }
}

// ── TileId conversions ────────────────────────────────────────────────────────
//
// Pane tiles use the lower 63 bits (bit 63 = 0).
// Container tiles use the lower 63 bits with bit 63 forced to 1.
// This guarantees the two namespaces never collide even when UUIDs are random.

pub fn pane_id_to_tile_id(id: PaneId) -> TileId {
    let bits = id.0.as_u64_pair().0 & !(1u64 << 63);
    TileId::from_u64(bits)
}

pub fn container_id_to_tile_id(id: ContainerId) -> TileId {
    let bits = id.0.as_u64_pair().0 | (1u64 << 63);
    TileId::from_u64(bits)
}

pub fn contents_to_tile_id(contents: Contents) -> TileId {
    match contents {
        Contents::Pane(id) => pane_id_to_tile_id(id),
        Contents::Container(id) => container_id_to_tile_id(id),
    }
}
