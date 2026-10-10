#!/usr/bin/env python3
# Phase 21.21 — deterministic RSS / Atom feed fixture generator (Python stdlib only).
# Exercises the surface the feed adapter claims: RSS 2.0 channel/`item` and Atom
# `feed`/`entry`; channel-level fields; entry field order; element attributes
# (attribute order + spelling); CDATA and character entities; duplicate entry
# elements; a namespaced element; and a multi-hundred-kilobyte document. Controls
# pin the detection boundaries.
#
# Entries deliberately contain only plain-text fields the adapter enumerates, so a
# conventional ElementTree walk and VOLE agree on the record field list; the
# entity/CDATA/attribute spelling differences are exercised as separate questions.
#
#   python3 tools/fixtures/make-feed.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def rss_basic():
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<rss version="2.0">\n'
        '  <channel>\n'
        '    <title>Example Feed</title>\n'
        '    <link>https://example.com/</link>\n'
        '    <description>demo</description>\n'
        '    <language>en</language>\n'
        '    <item>\n'
        '      <title>First</title>\n'
        '      <link>https://example.com/1</link>\n'
        '    </item>\n'
        '    <item>\n'
        '      <title>Second</title>\n'
        '      <link>https://example.com/2</link>\n'
        '    </item>\n'
        '  </channel>\n'
        '</rss>\n'
    ).encode("utf-8")


def rss_attrs():
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<rss version="2.0">\n'
        '  <channel>\n'
        '    <title>Attr Feed</title>\n'
        '    <link>https://example.com/</link>\n'
        '    <description>attrs</description>\n'
        '    <item>\n'
        '      <title>One</title>\n'
        '      <guid isPermaLink="false">urn:1</guid>\n'
        '      <category>alpha</category>\n'
        '      <category>beta</category>\n'
        '    </item>\n'
        '  </channel>\n'
        '</rss>\n'
    ).encode("utf-8")


def rss_cdata():
    return (
        '<?xml version="1.0"?>\n'
        '<rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/">\n'
        '  <channel>\n'
        '    <title>CDATA &amp; entities</title>\n'
        '    <link>https://example.com/</link>\n'
        '    <description><![CDATA[raw <b>markup</b> kept]]></description>\n'
        '    <item>\n'
        '      <title>Plain Title</title>\n'
        '      <link>https://example.com/c</link>\n'
        '    </item>\n'
        '  </channel>\n'
        '</rss>\n'
    ).encode("utf-8")


def atom_basic():
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<feed xmlns="http://www.w3.org/2005/Atom">\n'
        '  <title type="text">Atom Demo</title>\n'
        '  <id>urn:uuid:1</id>\n'
        '  <updated>2024-01-01T00:00:00Z</updated>\n'
        '  <link href="https://example.org/" rel="alternate"/>\n'
        '  <entry>\n'
        '    <title>Solo</title>\n'
        '    <id>urn:uuid:2</id>\n'
        '  </entry>\n'
        '</feed>\n'
    ).encode("utf-8")


def atom_multilink():
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en">\n'
        '  <title>Multi Entry</title>\n'
        '  <id>urn:uuid:10</id>\n'
        '  <updated>2024-02-02T00:00:00Z</updated>\n'
        '  <entry>\n'
        '    <title>E1</title>\n'
        '    <id>urn:uuid:11</id>\n'
        '  </entry>\n'
        '  <entry>\n'
        '    <title>E2</title>\n'
        '    <id>urn:uuid:12</id>\n'
        '  </entry>\n'
        '</feed>\n'
    ).encode("utf-8")


def dup_fields():
    return (
        '<?xml version="1.0"?>\n'
        '<rss version="2.0">\n'
        '  <channel>\n'
        '    <title>Dup</title>\n'
        '    <link>https://example.com/</link>\n'
        '    <description>d</description>\n'
        '    <item>\n'
        '      <title>T</title>\n'
        '      <category>a</category>\n'
        '      <category>b</category>\n'
        '      <category>c</category>\n'
        '    </item>\n'
        '  </channel>\n'
        '</rss>\n'
    ).encode("utf-8")


def large():
    out = []
    size = 0
    target = 256 * 1024
    i = 0
    out.append('<?xml version="1.0"?>\n<rss version="2.0"><channel>\n')
    out.append("<title>Large</title><link>https://x/</link><description>d</description>\n")
    while size < target:
        row = ("<item><title>item-%d</title><link>https://x/%d</link>"
               "<description>body %d</description></item>\n" % (i, i, i))
        out.append(row)
        size += len(row)
        i += 1
    out.append("</channel></rss>\n")
    return "".join(out).encode("utf-8")


# --- controls ---------------------------------------------------------------

def not_feed():
    return b'<?xml version="1.0"?>\n<root><a>1</a><b>2</b></root>\n'


def strict_json():
    return b'{"a": 1, "b": [2, 3]}\n'


def prose():
    return b"This is plain prose, not a feed document at all.\n"


def malformed():
    return b'<?xml version="1.0"?>\n<rss version="2.0"><channel><title>x</title>\n'


LANE = [
    ("rss-basic.xml", rss_basic),
    ("rss-attrs.xml", rss_attrs),
    ("rss-cdata.xml", rss_cdata),
    ("atom-basic.xml", atom_basic),
    ("atom-multilink.xml", atom_multilink),
    ("dup-fields.xml", dup_fields),
    ("large.xml", large),
]

CONTROLS = [
    ("notfeed.xml", not_feed),
    ("strict.json", strict_json),
    ("prose.txt", prose),
    ("malformed.xml", malformed),
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
    print("usage: make-feed.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
