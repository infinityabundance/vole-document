#!/usr/bin/env python3
# Phase 21.23 — deterministic KML / GPX (GIS family) fixture generator (Python
# stdlib only). Exercises the surface the GIS adapter claims: KML `<Document>` with
# `<Placemark>` points and `<Folder>` grouping; GPX `<wpt>`/`<rte>`/`<trk>` records;
# point coordinate spelling (KML `coordinates` text vs GPX `lat`/`lon` attributes);
# element namespace prefixes; `name`/`description` fields; duplicate record fields;
# and a multi-hundred-kilobyte document. Controls pin the detection boundaries.
#
#   python3 tools/fixtures/make-gis.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def kml_basic():
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<kml xmlns="http://www.opengis.net/kml/2.2">\n'
        '  <Document>\n'
        '    <name>Demo</name>\n'
        '    <Placemark>\n'
        '      <name>P1</name>\n'
        '      <Point><coordinates>1.0,2.0</coordinates></Point>\n'
        '    </Placemark>\n'
        '    <Placemark>\n'
        '      <name>P2</name>\n'
        '      <Point><coordinates>3.0,4.0</coordinates></Point>\n'
        '    </Placemark>\n'
        '  </Document>\n'
        '</kml>\n'
    ).encode("utf-8")


def kml_folder():
    return (
        '<?xml version="1.0"?>\n'
        '<kml xmlns="http://www.opengis.net/kml/2.2">\n'
        '  <Document>\n'
        '    <name>Folder Demo</name>\n'
        '    <Folder>\n'
        '      <name>Group</name>\n'
        '      <Placemark><name>N1</name>'
        '<Point><coordinates>-2.5e1,0.5</coordinates></Point></Placemark>\n'
        '    </Folder>\n'
        '    <Placemark id="p2"><name>N2</name>'
        '<Point><coordinates>1.50,3</coordinates></Point></Placemark>\n'
        '  </Document>\n'
        '</kml>\n'
    ).encode("utf-8")


def kml_dupfields():
    return (
        '<?xml version="1.0"?>\n'
        '<kml xmlns="http://www.opengis.net/kml/2.2">\n'
        '  <Document>\n'
        '    <name>D</name>\n'
        '    <Placemark>\n'
        '      <name>first</name>\n'
        '      <name>second</name>\n'
        '      <description>d</description>\n'
        '    </Placemark>\n'
        '  </Document>\n'
        '</kml>\n'
    ).encode("utf-8")


def gpx_basic():
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<gpx version="1.1" creator="demo" xmlns="http://www.topografix.com/GPX/1/1">\n'
        '  <metadata><name>Track</name></metadata>\n'
        '  <wpt lat="1.0" lon="2.0"><name>W1</name><ele>10.5</ele></wpt>\n'
        '  <trk><name>T</name><trkseg><trkpt lat="1.1" lon="2.1"/></trkseg></trk>\n'
        '</gpx>\n'
    ).encode("utf-8")


def gpx_routes():
    return (
        '<?xml version="1.0"?>\n'
        '<gpx version="1.1" creator="c" xmlns="http://www.topografix.com/GPX/1/1">\n'
        '  <metadata><name>Routes</name></metadata>\n'
        '  <rte><name>R1</name>'
        '<rtept lat="-2.500000" lon="0.5"/><rtept lat="1.50" lon="3.0"/></rte>\n'
        '  <wpt lat="9.0" lon="9.0"><name>W</name><ele>1</ele></wpt>\n'
        '</gpx>\n'
    ).encode("utf-8")


def large():
    out = []
    size = 0
    target = 256 * 1024
    i = 0
    out.append('<?xml version="1.0"?>\n<kml xmlns="http://www.opengis.net/kml/2.2">\n<Document>\n<name>L</name>\n')
    while size < target:
        row = ('<Placemark><name>p-%d</name>'
               '<Point><coordinates>%d.0,%d.0</coordinates></Point></Placemark>\n' % (i, i, i))
        out.append(row)
        size += len(row)
        i += 1
    out.append("</Document></kml>\n")
    return "".join(out).encode("utf-8")


# --- controls ---------------------------------------------------------------

def not_gis():
    return b'<?xml version="1.0"?>\n<root><a>1</a><b>2</b></root>\n'


def strict_json():
    return b'{"a": 1, "b": [2, 3]}\n'


def prose():
    return b"Plain prose, not a KML or GPX document at all.\n"


def malformed():
    return b'<?xml version="1.0"?>\n<kml><Document><name>x</name>\n'


LANE = [
    ("placemarks.kml", kml_basic),
    ("folder.kml", kml_folder),
    ("dupfields.kml", kml_dupfields),
    ("track.gpx", gpx_basic),
    ("routes.gpx", gpx_routes),
    ("large.kml", large),
]

CONTROLS = [
    ("notgis.xml", not_gis),
    ("strict.json", strict_json),
    ("prose.txt", prose),
    ("malformed.kml", malformed),
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
    print("usage: make-gis.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
