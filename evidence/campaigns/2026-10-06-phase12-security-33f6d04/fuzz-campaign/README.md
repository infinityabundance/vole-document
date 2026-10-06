# Phase-12.13 fuzz campaign (fuzz service: nightly-2026-10-04, cargo-fuzz 0.13.2, libfuzzer-sys 0.4.13, mem 4g, 4 cpus)
#
# Bounded campaign, FUZZ_SECONDS=10, FUZZ_RSS_MB=2048. Every Phase-12 target
# (zip_scan, zip_decode, opc_rels, docx_wml, epub_package, epub_content,
# xml_part, common_observe) finished exit=0 with 0 crash/OOM/timeout artifacts.
#
# The only artifact of the whole 18-target campaign was deflate_replay
# (oom-cba63e89...): the pre-existing upstream preflate-rs 0.7.6 F2
# resource limitation already recorded in fuzz/README.md (ADR-0016), not a
# Phase-12 surface and not a new finding.
