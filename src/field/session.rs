//! Resident field runtime (Phase 15.2): open the store and field ONCE and serve
//! many observations in one process. Observe-only: it is off the exactness path
//! and never backs `materialize --exact`.

use std::path::Path;

#[cfg(not(feature = "entropyfs-store"))]
use crate::error::Error;
use crate::error::Result;
use crate::limits::Limits;
use crate::store::IoSnapshot;

use super::index::FsIndexStore;
use super::observe::{
    ModelMemo, ObserveRequest, ObserveStats, observe_session, open_session_index,
};
use super::promote::PromotePolicy;
use super::provenance::FieldAnswer;
use super::{Field, FieldId, FieldStore};

/// Default in-memory typed-model memo budget (bytes of materialized node output).
pub const DEFAULT_MODEL_MEMO_BYTES: u64 = 64 * 1024 * 1024;

/// Options for opening a [`DocumentFieldSession`].
#[derive(Debug, Clone, Copy)]
pub struct SessionOptions {
    /// Open the EntropyFS backend instead of the filesystem backend.
    pub entropyfs: bool,
    /// Open the packed seed substrate (`fieldpack/`) instead of `seed/`.
    /// Mutually exclusive with `entropyfs`.
    pub packed: bool,
    /// In-memory typed-model memo budget; `0` disables the memo.
    pub model_memo_bytes: u64,
    /// Opt-in durable promotion policy (Phase 15.6); disabled by default.
    pub promote: PromotePolicy,
}

impl Default for SessionOptions {
    fn default() -> Self {
        SessionOptions {
            entropyfs: false,
            packed: false,
            model_memo_bytes: DEFAULT_MODEL_MEMO_BYTES,
            promote: PromotePolicy::default(),
        }
    }
}

/// One process serving many observations against one field.
///
/// The store and field are opened **once** ([`open`]); every [`observe`] then
/// reuses that open field and the derived-cache probe, so manifest/descriptor
/// reads, their content-id BLAKE3 checks, the descriptor parse, and the typed
/// model decodes are paid once rather than per observation.
///
/// [`open`]: DocumentFieldSession::open
/// [`observe`]: DocumentFieldSession::observe
pub struct DocumentFieldSession {
    store: FieldStore,
    field: Field,
    field_id: FieldId,
    /// The observation index store, opened **once** alongside the field and kept
    /// open across observations so the cache-first probe never re-opens it.
    index: FsIndexStore,
    models: ModelMemo,
    /// The one-time open cost, attributed to the first observation (so the sum
    /// over a batch is honest), then dropped.
    pending_open_io: Option<IoSnapshot>,
}

impl DocumentFieldSession {
    /// Open the store and the field once. Does the manifest read + content_id
    /// BLAKE3, descriptor read + Id::of BLAKE3 + Descriptor::parse, and the
    /// FieldId hex parse exactly once.
    pub fn open(store_dir: &Path, field_hex: &str, opts: SessionOptions) -> Result<Self> {
        let prof = super::warm_prof_enabled();
        let t0 = std::time::Instant::now();
        let mut store = if opts.packed {
            FieldStore::open_packed(store_dir)?
        } else if opts.entropyfs {
            #[cfg(feature = "entropyfs-store")]
            {
                FieldStore::open_entropyfs(store_dir)?
            }
            #[cfg(not(feature = "entropyfs-store"))]
            {
                return Err(Error::unsupported_feature(
                    "--entropyfs requires a build with the entropyfs-store feature",
                ));
            }
        } else {
            FieldStore::open(store_dir)?
        };
        store.set_promote(opts.promote);
        let t_store = t0.elapsed();
        let field_id = FieldId::from_hex(field_hex)?;
        let field = Field::open(&store, &field_id, Limits::DEFAULT)?;
        let open_io = field.open_io();
        let t_field = t0.elapsed();
        // Open the index store once; the probe path borrows it for every request.
        let index = open_session_index(&store)?;
        let t_index = t0.elapsed();
        if prof {
            eprintln!(
                "[vole-profile] store_open_us={} field_open_us={} index_open_us={} session_open_total_us={}",
                t_store.as_micros(),
                t_field.as_micros().saturating_sub(t_store.as_micros()),
                t_index.as_micros().saturating_sub(t_field.as_micros()),
                t_index.as_micros()
            );
        }
        Ok(Self {
            store,
            field,
            field_id,
            index,
            models: ModelMemo::with_budget(opts.model_memo_bytes),
            pending_open_io: Some(open_io),
        })
    }

    /// The field this session serves.
    pub fn field_id(&self) -> FieldId {
        self.field_id
    }

    /// One observation. `&mut self.store`, `&self.field`, `&self.index`, and a
    /// clone of the `Rc`-based memo are disjoint field borrows in one call.
    pub fn observe(
        &mut self,
        req: &ObserveRequest,
        limits: Limits,
    ) -> Result<(FieldAnswer, ObserveStats, FieldId)> {
        let open_io = self.pending_open_io.take().unwrap_or_default();
        observe_session(
            &mut self.store,
            &self.field,
            &self.index,
            open_io,
            req,
            limits,
            self.models.clone(),
        )
    }

    /// N observations in one process, in order.
    pub fn observe_batch(
        &mut self,
        reqs: &[ObserveRequest],
        limits: Limits,
    ) -> Vec<Result<(FieldAnswer, ObserveStats, FieldId)>> {
        reqs.iter().map(|r| self.observe(r, limits)).collect()
    }

    /// EntropyFS epoch barrier; no-op on the filesystem backend.
    pub fn sync(&self) -> Result<()> {
        self.store.sync()
    }
}
