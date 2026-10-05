//! The `.voldoc` container: header, framing, and descriptor model.

pub mod descriptor;
pub mod header;
pub mod observation;
pub mod record;

pub use descriptor::{Descriptor, ParsedDescriptor, UNIVERSE, universe_id_from_str};
pub use header::{HEADER_LEN, Header, MAGIC};
pub use observation::{ObservationDigest, ObservationIndex, ObservationSelector, OpEntry};
pub use record::{RECORD_OVERHEAD, Record, RecordReader, RecordTag};
