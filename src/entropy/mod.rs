//! Entropy substrate: canonical models, channel descriptors, and (later) rANS.

pub mod codec;
pub mod model;
#[cfg(feature = "rans")]
pub mod rans;

pub use codec::{CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor};
pub use model::{ALPHABET, EntropyModel, MODEL_VERSION_1};
#[cfg(feature = "rans")]
pub use rans::{Capsule, decode_channel, encode_channel};
