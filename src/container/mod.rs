//! The `.voldoc` container: header, framing, and descriptor model.

pub mod checkpoint;
pub mod descriptor;
pub mod directory;
pub mod header;
pub mod observation;
pub mod record;

pub use checkpoint::{
    CHECKPOINT_KIND_OP_BOUNDARIES, CHECKPOINT_VERSION, CheckpointEntry, CheckpointTable,
};
pub use descriptor::{
    Descriptor, EXTERNAL_REF_PAYLOAD_LEN, ObjectSource, ParsedDescriptor, UNIVERSE,
    universe_id_from_str,
};
pub use directory::{
    ClassEntry, DirectoryEntry, RecordSite, SEEK_DIRECTORY_VERSION, SeekDirectory, class_index,
};
pub use header::{
    FEATURE_CHECKPOINTS, FEATURE_EXTERNAL_OBJECTS, FEATURE_SEEK_DIRECTORY, HEADER_LEN, Header,
    MAGIC,
};
pub use observation::{ObservationDigest, ObservationIndex, ObservationSelector, OpEntry};
pub use record::{RECORD_OVERHEAD, Record, RecordReader, RecordTag};
