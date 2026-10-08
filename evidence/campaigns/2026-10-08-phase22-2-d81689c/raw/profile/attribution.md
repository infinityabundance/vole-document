## Per-document attribution (packed store, one warm session)

| document | session us | open % | Descriptor::parse % | loop % | probe % | index-read % | dispatch % | materialize % | serialize % | idx openat/reads | distinct idx nodes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nist-pdf-0002 | 728 | 82.7 | 63.5 | 17.3 | 3.85 | 3.6 | 6.2 | 1.9 | 1.4 | 4/4 | 3 |
| nist-pdf-0004 | 2431 | 95.6 | 78.0 | 4.4 | 0.99 | 0.9 | 1.4 | 0.5 | 0.3 | 4/4 | 3 |
| nist-pdf-0016 | 737 | 88.3 | 71.5 | 11.7 | 2.99 | 2.2 | 3.0 | 0.9 | 1.1 | 4/4 | 3 |
| nist-pdf-0017 | 3250 | 94.4 | 77.5 | 5.6 | 0.77 | 1.0 | 3.2 | 0.9 | 0.3 | 4/4 | 3 |
| nist-docx-0005 | 605 | 27.3 | 16.7 | 72.7 | 0.00 | 9.9 | 38.7 | 5.5 | 18.8 | 24/24 | 1 |
| nist-docx-0008 | 388 | 27.3 | 13.9 | 72.7 | 0.00 | 15.7 | 36.6 | 5.2 | 11.1 | 24/24 | 1 |
| nist-docx-0009 | 1031 | 32.9 | 25.1 | 67.1 | 0.00 | 10.0 | 41.6 | 4.4 | 14.3 | 24/24 | 1 |
| nist-docx-0014 | 3013 | 65.3 | 53.8 | 34.7 | 0.00 | 4.2 | 22.3 | 2.1 | 10.3 | 24/24 | 1 |
| nist-epub-0003 | 2346 | 9.7 | 6.7 | 90.3 | 0.00 | 22.9 | 70.8 | 15.5 | 16.4 | 114/114 | 1 |
| nist-epub-0006 | 1654 | 15.9 | 11.2 | 84.1 | 0.00 | 24.1 | 70.3 | 16.6 | 10.0 | 90/90 | 1 |
| nist-epub-0008 | 840 | 15.2 | 8.2 | 84.8 | 0.00 | 23.6 | 71.5 | 18.8 | 6.3 | 60/60 | 1 |
| nist-epub-0009 | 5196 | 48.0 | 40.3 | 52.0 | 0.00 | 15.0 | 38.7 | 7.3 | 11.9 | 117/117 | 1 |

## Pooled (sum of one warm session per document, packed)

- open_us: **12334 us** (55.5% of 22219 us)
- descriptor_parse_us: **9941 us** (44.7% of 22219 us)
- loop_us: **9885 us** (44.5% of 22219 us)
- dispatch_us: **7120 us** (32.0% of 22219 us)
- materialize_us: **1397 us** (6.3% of 22219 us)
- serialize_us: **1868 us** (8.4% of 22219 us)
- probe_us: **99 us** (0.4% of 22219 us)
- index_read_us: **2327 us** (10.5% of 22219 us)
- index_parse_us: **33 us** (0.1% of 22219 us)
- index nodes read / index descents: **493** / **485**
- redundant re-reads: **493** index opens for **20** distinct index node files across the 12 sessions

## Selector-directory headroom (upper bound)

- pooled index share: **10.6%** of the warm session; median per-document share: **10.0%**.
- A PERFECT selector directory can remove at most this share (it still reads offsets and still dispatches/materializes/serializes).
- Implied headline-ratio shift: `ratio * (1 - share)` = a change of about **0.126** (pooled) / 0.119 (median doc) on the current warm median 1.189 vs the tuned `full` envelope.
- The court's minimum detectable effect at N=100 is **~0.399**. A perfect selector directory is therefore **below the MDE** and not resolvable by this court.

## Packed vs fs contrast (same request set)

| document | backend | session us | index-read us | store bytes |
|---|---|---:|---:|---:|
| nist-pdf-0004 | packed | 2431 | 20 | 1216084 |
| nist-pdf-0004 | fs | 2331 | 17 | 1180216 |
| nist-docx-0008 | packed | 388 | 60 | 37639 |
| nist-docx-0008 | fs | 374 | 63 | 33915 |
| nist-epub-0009 | packed | 5196 | 765 | 1259371 |
| nist-epub-0009 | fs | 4996 | 731 | 1252111 |
