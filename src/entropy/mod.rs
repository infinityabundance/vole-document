//! Entropy substrate: canonical models, channel descriptors, and (later) rANS.

pub mod model;
pub mod rans;

pub use model::{ALPHABET, EntropyModel, MODEL_VERSION_1};
pub use rans::{Capsule, decode_channel, encode_channel};
