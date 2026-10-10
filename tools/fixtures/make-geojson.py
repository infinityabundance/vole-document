#!/usr/bin/env python3
# Phase 21.22 — deterministic GeoJSON (RFC 7946) fixture generator (Python stdlib
# only). Exercises the surface the GeoJSON adapter claims: a `FeatureCollection`
# and a bare `Feature`; Point/LineString/Polygon/GeometryCollection; exact numeric
# coordinate spelling; `properties` member order and duplicate keys; foreign
# (non-core) members preserved; `id` and `bbox`; and a multi-hundred-kilobyte
# document. Controls pin the detection boundaries.
#
#   python3 tools/fixtures/make-geojson.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def points():
    return (
        '{\n'
        '  "type": "FeatureCollection",\n'
        '  "features": [\n'
        '    {"type": "Feature",\n'
        '     "properties": {"name": "A", "value": 1.50, "a": 1, "a": 2},\n'
        '     "geometry": {"type": "Point", "coordinates": [102.0, 0.5]}},\n'
        '    {"type": "Feature",\n'
        '     "properties": {"name": "B"},\n'
        '     "geometry": {"type": "LineString", "coordinates": [[102.0, 0.0], [103.0, 1.0]]}}\n'
        '  ],\n'
        '  "foreign": {"kept": true}\n'
        '}\n'
    ).encode("utf-8")


def feature():
    return (
        '{\n'
        '  "type": "Feature",\n'
        '  "properties": {"name": "solo"},\n'
        '  "geometry": {"type": "Point", "coordinates": [1.0, 2.0]}\n'
        '}\n'
    ).encode("utf-8")


def coords():
    return (
        '{"type": "Feature", "properties": {"n": 1},\n'
        ' "geometry": {"type": "LineString", '
        '"coordinates": [[-2.5e1, 0.5], [1.50, 3]]}}\n'
    ).encode("utf-8")


def foreign():
    return (
        '{"type": "Feature", "id": "f-1", "bbox": [0, 0, 1, 1],\n'
        ' "vendor": {"x": 1}, "properties": {"name": "F"},\n'
        ' "geometry": {"type": "Point", "coordinates": [0.0, 0.0]}}\n'
    ).encode("utf-8")


def geomcoll():
    return (
        '{"type": "Feature", "properties": {"name": "G"},\n'
        ' "geometry": {"type": "GeometryCollection",\n'
        '   "geometries": [\n'
        '     {"type": "Point", "coordinates": [1.0, 1.0]},\n'
        '     {"type": "Point", "coordinates": [2.0, 2.0]}\n'
        '   ]}}\n'
    ).encode("utf-8")


def dupkeys():
    return (
        '{"type": "Feature", "properties": {"k": 1, "k": 2, "k": 3, "z": 9},\n'
        ' "geometry": {"type": "Point", "coordinates": [0.0, 0.0]}}\n'
    ).encode("utf-8")


def large():
    feats = []
    target = 256 * 1024
    size = 0
    i = 0
    while size < target:
        row = ('{"type": "Feature", "properties": {"name": "item-%d", "value": %d},\n'
               ' "geometry": {"type": "Point", "coordinates": [%d.5, %d.0]}}'
               % (i, i, i, i))
        feats.append(row)
        size += len(row)
        i += 1
    body = ('{"type": "FeatureCollection", "features": [' + ",".join(feats) + "]}\n")
    return body.encode("utf-8")


# --- controls ---------------------------------------------------------------

def not_geojson():
    return b'{"a": 1, "b": [2, 3]}\n'


def wrong_type():
    return b'{"type": "NotGeo", "a": 1}\n'


def malformed():
    return b'{"type": "Feature", "properties": {'


def prose():
    return b"Plain prose, not a GeoJSON document at all.\n"


LANE = [
    ("points.geojson", points),
    ("feature.geojson", feature),
    ("coords.geojson", coords),
    ("foreign.geojson", foreign),
    ("geomcoll.geojson", geomcoll),
    ("dupkeys.geojson", dupkeys),
    ("large.geojson", large),
]

CONTROLS = [
    ("notgeojson.json", not_geojson),
    ("wrongtype.json", wrong_type),
    ("malformed.geojson", malformed),
    ("prose.txt", prose),
]

FIXTURES = LANE + CONTROLS


def emit_corpus(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    for name, fn in FIXTURES:
        data = fn()
        with open(os.path.join(out_dir, name), "wb") as f:
            f.write(data)
        print("%s\t%d\t%s" % (name, len(data), hashlib.sha256(data).hexdigest()))


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "--corpus":
        emit_corpus(sys.argv[2])
        return 0
    print("usage: make-geojson.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
