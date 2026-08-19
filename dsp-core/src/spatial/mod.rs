pub mod referencing;
pub mod whitening;

pub use referencing::{apply_car, apply_cmr};
pub use whitening::{apply_whitening, estimate_covariance};
