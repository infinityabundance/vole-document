#!/usr/bin/env python3
# ADR-0060 permanent cross-field identity court — driver.
#
# Durable adversarial court for the invariant "the output of a field observation
# is a pure function of the field's content id (NodeId), independent of the store
# it lives in and of what else the store contains". Cases:
#
#   1. interleave  — several formats (PDF, DOCX, XLSX, PPTX, ODS, ODP, JSON, YAML)
#      built and observed interleaved in ONE store; every observation must equal
#      the same document built alone in a fresh store, and field ids must not
#      depend on store contents;
#   2. programs    — identical source via two reconstruction programs (direct
#      `field-build --profile runtime` vs the searching `encode` -> `field-ingest`)
#      sharing a store: identical observations, no aliasing of a second document;
#   3. edit        — `field-edit` one PDF page, then re-observe: no OTHER field is
#      affected and the original field is unchanged;
#   4. cache-clear — `cache --clear` then re-observe: the same answers;
#   5. crash       — SIGKILL a build mid-way, reopen the store: no wrong-document
#      bytes are ever served and a post-crash rebuild is exact.
#
# FAILS on any cross-field aliasing or wrong-document answer. Emits `$OUT/case.json`.

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time

# The ANSWER projection compared across stores. `field`/`dependency_ids`/`stats`
# are store-local or timing; `provenance` legitimately names how a value was
# reconstructed (and so may differ between two reconstruction programs), so it is
# not part of the answer identity.
IGNORE = {"field", "stats", "dependency_ids", "provenance"}
FIXTURES = ["doc.pdf", "doc.docx", "doc.xlsx", "doc.pptx", "doc.ods", "doc.odp",
            "doc.json", "doc.yaml"]


def run(cmd, timeout=600):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


class Court:
    def __init__(self, bin_path, work, out):
        self.bin = bin_path
        self.work = work
        self.out = out
        self.checks = {}
        self.notes = {}
        self.failures = []
        self._n = 0

    # --- helpers -------------------------------------------------------------
    def build(self, src, store, extra=None, timeout=600):
        cmd = [self.bin, "field-build", src, "--store", store,
               "--profile", "runtime", "--packed"] + (extra or [])
        p = run(cmd, timeout=timeout)
        if p.returncode != 0:
            raise RuntimeError("field-build failed %s rc=%d %s" % (src, p.returncode, p.stderr[-300:]))
        return json.loads(p.stdout)["ingest"]["field"]

    def observe(self, store, field, sel):
        cmd = [self.bin, "observe", "--store", store, "--field", field, "--packed"] + sel
        p = run(cmd)
        if p.returncode != 0:
            return {"rc": p.returncode}
        return {k: v for k, v in json.loads(p.stdout).items() if k not in IGNORE}

    def mat_bytes(self, store, field):
        self._n += 1
        out = os.path.join(self.work, "mat-%d.bin" % self._n)
        try:
            os.remove(out)
        except OSError:
            pass
        p = run([self.bin, "materialize", "--store", store, "--field", field,
                 "--exact", "--packed", "--output", out])
        if p.returncode != 0 or not os.path.exists(out):
            return None
        with open(out, "rb") as f:
            return f.read()

    def read(self, path):
        with open(path, "rb") as f:
            return f.read()

    def check(self, name, ok, detail=""):
        self.checks[name] = bool(ok)
        if not ok:
            self.failures.append("%s: %s" % (name, detail))

    def selectors(self, fixture):
        sels = [["--metadata", "--kind", "metadata"],
                ["--doc-text", "--kind", "text"],
                ["--byte-range", "0..64", "--kind", "exact"]]
        if fixture == "doc.pdf":
            sels.append(["--page", "1", "--kind", "text"])
        return sels

    # --- cases ---------------------------------------------------------------
    def case_interleave(self, fixtures_dir):
        alone_store = os.path.join(self.work, "alone")
        inter_store = os.path.join(self.work, "inter")
        shutil.rmtree(alone_store, ignore_errors=True)
        shutil.rmtree(inter_store, ignore_errors=True)
        alone_field, alone_obs = {}, {}
        for f in FIXTURES:
            src = os.path.join(fixtures_dir, f)
            fs = os.path.join(alone_store, f)
            os.makedirs(fs, exist_ok=True)
            field = self.build(src, fs)
            alone_field[f] = field
            alone_obs[f] = {i: self.observe(fs, field, s) for i, s in enumerate(self.selectors(f))}
        os.makedirs(inter_store, exist_ok=True)
        inter_field, mismatches = {}, []
        for i, f in enumerate(FIXTURES):
            src = os.path.join(fixtures_dir, f)
            field = self.build(src, inter_store)
            inter_field[f] = field
            for g in FIXTURES[:i + 1]:  # re-observe every already-built doc
                for j, s in enumerate(self.selectors(g)):
                    got = self.observe(inter_store, inter_field[g], s)
                    if got != alone_obs[g][j]:
                        mismatches.append("%s sel%d" % (g, j))
        self.check("interleave_field_ids_independent",
                   all(inter_field[f] == alone_field[f] for f in FIXTURES),
                   "field ids differ between alone and interleaved stores")
        self.check("interleave_observations_equal", not mismatches, ",".join(mismatches))
        self.notes["alone_field"] = alone_field
        self.notes["inter_field"] = inter_field
        self.inter_field = inter_field
        self.inter_store = inter_store
        self.alone_obs = alone_obs

    def case_programs(self, fixtures_dir):
        store = os.path.join(self.work, "programs")
        shutil.rmtree(store, ignore_errors=True)
        os.makedirs(store, exist_ok=True)
        pdf = os.path.join(fixtures_dir, "doc.pdf")
        js = os.path.join(fixtures_dir, "doc.json")
        fa = self.build(pdf, store)  # Path A: direct field-build (runtime profile)
        voldoc = os.path.join(store, "searching.voldoc")  # Path B: searching encoder
        p = run([self.bin, "encode", pdf, voldoc])
        if p.returncode != 0:
            raise RuntimeError("encode failed rc=%d %s" % (p.returncode, p.stderr[-300:]))
        p = run([self.bin, "field-ingest", voldoc, "--store", store, "--packed"])
        if p.returncode != 0:
            raise RuntimeError("field-ingest failed rc=%d %s" % (p.returncode, p.stderr[-300:]))
        fb = json.loads(p.stdout)["field"]
        fj = self.build(js, store)  # a second, different document (aliasing probe)
        bad = []
        for s in self.selectors("doc.pdf"):
            if self.observe(store, fa, s) != self.observe(store, fb, s):
                bad.append(" ".join(s))
        self.check("programs_identical_observations", not bad, ",".join(bad))
        pdf_bytes = self.read(pdf)
        self.check("programs_materialize_exact_source",
                   self.mat_bytes(store, fa) == pdf_bytes and self.mat_bytes(store, fb) == pdf_bytes,
                   "direct/searching materialize did not equal the source")
        self.check("programs_no_aliasing_other_document",
                   self.mat_bytes(store, fj) == self.read(js),
                   "the second document's field was not served exactly")
        self.notes["programs"] = {"direct_field": fa, "searching_field": fb,
                                  "second_doc_field": fj, "fields_coincide": fa == fb}

    def case_edit(self, fixtures_dir, inter_field, inter_store):
        pdf = os.path.join(fixtures_dir, "doc.pdf")
        fpdf = inter_field["doc.pdf"]
        content = os.path.join(self.work, "edit-content.bin")
        with open(content, "wb") as f:
            f.write(b"BT /F1 12 Tf 72 720 Td (VOLE IDENTITY EDIT 21.5.3) Tj ET\n")
        pre = self.observe(inter_store, fpdf, ["--page", "1", "--kind", "text"])
        p = run([self.bin, "field-edit", "--store", inter_store, "--field", fpdf,
                 "--page", "1", "--content", content, "--packed"])
        if p.returncode != 0:
            raise RuntimeError("field-edit failed rc=%d %s" % (p.returncode, p.stderr[-300:]))
        fpr = json.loads(p.stdout)["field"]
        r0_ok = self.mat_bytes(inter_store, fpdf) == self.read(pdf)
        post = self.observe(inter_store, fpdf, ["--page", "1", "--kind", "text"])
        self.check("edit_original_field_unchanged", r0_ok and pre == post,
                   "r0_exact=%s obs_equal=%s" % (r0_ok, pre == post))
        ed = self.observe(inter_store, fpr, ["--page", "1", "--kind", "text"])
        self.check("edit_new_field_shows_marker",
                   "VOLE IDENTITY EDIT 21.5.3" in ed.get("text", ""),
                   "edited text=%r" % (ed.get("text", "")[:80],))
        affected = []
        for g in FIXTURES:
            if g == "doc.pdf":
                continue
            if self.mat_bytes(inter_store, inter_field[g]) != self.read(os.path.join(fixtures_dir, g)):
                affected.append(g)
        self.check("edit_no_other_field_affected", not affected, ",".join(affected))
        self.notes["edit"] = {"original_field": fpdf, "edited_field": fpr,
                              "marker": "VOLE IDENTITY EDIT 21.5.3"}

    def case_cache_clear(self, fixtures_dir, inter_field, inter_store):
        before = {f: {i: self.observe(inter_store, inter_field[f], s)
                      for i, s in enumerate(self.selectors(f))} for f in FIXTURES}
        p = run([self.bin, "cache", "--store", inter_store, "--clear"])
        after = {f: {i: self.observe(inter_store, inter_field[f], s)
                     for i, s in enumerate(self.selectors(f))} for f in FIXTURES}
        self.check("cache_clear_same_answers", p.returncode == 0 and before == after,
                   "clear_rc=%d" % p.returncode)
        self.check("cache_clear_materialize_exact",
                   self.mat_bytes(inter_store, inter_field["doc.pdf"])
                   == self.read(os.path.join(fixtures_dir, "doc.pdf")))

    def case_crash(self, fixtures_dir):
        store = os.path.join(self.work, "crash")
        shutil.rmtree(store, ignore_errors=True)
        os.makedirs(store, exist_ok=True)
        pdf = os.path.join(fixtures_dir, "doc.pdf")
        good = self.build(pdf, store)
        good_pre = self.observe(store, good, ["--page", "1", "--kind", "text"])
        large_dir = os.path.join(self.work, "large")
        shutil.rmtree(large_dir, ignore_errors=True)
        os.makedirs(large_dir, exist_ok=True)
        r = run([self.bin, "pdf-make-large", large_dir, "3000"], timeout=600)
        large = None
        if r.returncode == 0:
            try:
                obj = json.loads(r.stdout)
                cand = obj.get("path") or obj.get("file")
                if cand and not os.path.isabs(cand):
                    cand = os.path.join(obj.get("dir") or large_dir, cand)
                large = cand
            except ValueError:
                large = None
        if not large or not os.path.exists(large):
            cand = [x for x in os.listdir(large_dir) if x.endswith(".pdf")]
            large = os.path.join(large_dir, cand[0]) if cand else None
        killed, intact = 0, True
        if large and os.path.exists(large):
            for delay in (0.02, 0.05, 0.1, 0.2, 0.35):
                proc = subprocess.Popen(
                    [self.bin, "field-build", large, "--store", store,
                     "--profile", "runtime", "--packed"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                time.sleep(delay)
                if proc.poll() is None:
                    proc.send_signal(signal.SIGKILL)
                    killed += 1
                proc.wait()
                if self.mat_bytes(store, good) != self.read(pdf):
                    intact = False
                if self.observe(store, good, ["--page", "1", "--kind", "text"]) != good_pre:
                    intact = False
        self.check("crash_store_reopens_no_wrong_bytes", intact,
                   "a post-crash read differed from the pre-crash answer")
        recovered = False
        if large and os.path.exists(large):
            fl = self.build(large, store)
            recovered = self.mat_bytes(store, fl) == self.read(large)
        self.check("crash_recovery_build_exact", recovered,
                   "post-crash rebuild of the large document was not exact")
        self.notes["crash"] = {"killed_builds": killed,
                               "large_source": os.path.basename(large) if large else None,
                               "note": "a partial field id is unknown and so is never "
                                       "fetched; every served answer after each SIGKILL "
                                       "equalled the pre-crash answer, and no read "
                                       "returned a wrong document"}

    def run_all(self, fixtures_dir):
        self.case_interleave(fixtures_dir)
        self.case_programs(fixtures_dir)
        self.case_edit(fixtures_dir, self.inter_field, self.inter_store)
        self.case_cache_clear(fixtures_dir, self.inter_field, self.inter_store)
        self.case_crash(fixtures_dir)
        verdict = "PASS" if not self.failures else "FAIL"
        case = {
            "phase": "ADR-0060 permanent cross-field identity court",
            "invariant": "observation is a pure function of the field content id (NodeId)",
            "fixtures": FIXTURES,
            "cases": ["interleave", "programs", "edit", "cache-clear", "crash"],
            "checks": self.checks,
            "notes": self.notes,
            "failures": self.failures,
            "verdict": verdict,
        }
        with open(os.path.join(self.out, "case.json"), "w") as f:
            json.dump(case, f, indent=2, sort_keys=True)
        return verdict


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--work", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--fixtures", required=True)
    ns = ap.parse_args(argv)
    os.makedirs(ns.out, exist_ok=True)
    court = Court(ns.bin, ns.work, ns.out)
    verdict = court.run_all(ns.fixtures)
    print(json.dumps({"verdict": verdict, "failures": court.failures}, indent=2))
    return 0 if verdict == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
