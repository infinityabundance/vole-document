//! Entropy substrate: canonical models, channel descriptors, and (later) rANS.

pub mod codec;
pub mod model;
pub mod rans;

pub use codec::{CODER_ORDER0_BYTE_RANS, CODER_VERSION_1, EntropyChannelDescriptor};
pub use model::{ALPHABET, EntropyModel, MODEL_VERSION_1};
pub use rans::{Capsule, decode_channel, encode_channel};
