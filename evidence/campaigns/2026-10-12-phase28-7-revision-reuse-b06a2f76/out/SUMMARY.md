# Phase 28.7 — Experiment 3: cross-revision dependency reuse (measurement only)

Simulation. No wire format, no planner/decoder change, no production-storage edit.

Question: on authentic revision families, how much *verified work* is
reusable under a fragment-level dependency graph, what index does it need,
and does it beat a full rebuild and a versioned content-addressed store?

Keys: **L1 full** = rebuild each revision from scratch; **L2 cas** =
fragment-level content-addressed store (content-defined chunking, dedup by
hash, no semantic edges -> must recompute every observation); **L3 graph** =
L2 + semantic dependency edges -> reuse an observation whose full closure is
unchanged. Work is counted in verified node executions; latency is modeled.

Identities are kept separate (ADR-0060): the exact revision SHA-256, the
physical fragment content id, the semantic context signature, and the derived
observation content id are distinct namespaces. Context (Phase 15.6): the
existing content-addressed descriptor store retained ~0 cross-revision work,
because 97.2% of persistent bytes is the whole exact descriptor (Phase 22.6)
and any change invalidates it wholesale -- fragmenting the authority is what
localizes a revision.

## Per-family reuse (cumulative over the family)

  family        fmt   change          L1 raw B   L2 raw B   L3 raw B  L1 obs  L2 obs  L3 obs  L3 reused
  nist-sp800-34 docx  real-draft-edit    184,954    171,586    171,586   1,546   1,546   1,865          0
      persistent B: L1(raw)=184,954 L1(encoded store, measured)=225,402  L2=173,554  L3=225,394  (L3 index=53,808)
      latency us:  L1(measured)=96,831  L2(modeled)=95,924  L3(modeled)=115,205
  nist-sp800-18 docx  real-draft-edit  9,053,942  8,991,011  8,991,011     538     538     576          0
      persistent B: L1(raw)=9,053,942 L1(encoded store, measured)=9,203,854  L2=8,997,155  L3=9,023,363  (L3 index=32,352)
      latency us:  L1(measured)=193,925  L2(modeled)=181,651  L3(modeled)=192,015
  docx-style    docx  style-table       169,576     73,176     73,176   1,443   1,443   1,443          0
      persistent B: L1(raw)=169,576 L1(encoded store, measured)=199,912  L2=74,040  L3=91,032  (L3 index=17,856)
      latency us:  L1(measured)=70,838  L2(modeled)=69,693  L3(modeled)=69,693
  xlsx-ss       xlsx  shared-string      30,732     14,850     14,850      48      48      40          8
      persistent B: L1(raw)=30,732 L1(encoded store, measured)=59,802  L2=15,666  L3=16,258  (L3 index=1,408)
      latency us:  L1(measured)=66,218  L2(modeled)=49,473  L3(modeled)=43,384
  csv-text      csv   text-edit      12,582,964  4,260,449  4,260,449 389,208 389,208 131,957    257,322
      persistent B: L1(raw)=12,582,964 L1(encoded store, measured)=12,593,236  L2=4,275,041  L3=6,364,113  (L3 index=2,103,664)
      latency us:  L1(measured)=292,530  L2(modeled)=292,087  L3(modeled)=99,179
  json-text     json  text-edit      12,583,099  8,415,183  8,415,183 173,459 173,459 116,067     57,395
      persistent B: L1(raw)=12,583,099 L1(encoded store, measured)=12,593,272  L2=8,435,727  L3=9,366,575  (L3 index=951,392)
      latency us:  L1(measured)=555,191  L2(modeled)=554,250  L3(modeled)=371,318

## Totals across families

  metric                         L1 full          L2 cas          L3 graph
  raw bytes written              34,605,267      21,926,255      21,926,255
  persistent bytes               34,605,267      21,971,183      25,086,735
  verified obs executions           566,242         566,242         251,948
  obs available                     566,242         566,242         566,242
  index bytes                             0          44,928       3,160,480
  latency us                      1,275,533       1,243,078         890,794

## Exactness

  materialize --exact == source (length + SHA-256 + cmp), all revisions: 20/20

