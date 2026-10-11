#!/usr/bin/env python3
# Phase 21.30 — deterministic API/specification fixture generator for the ECONOMIC
# court (Python stdlib only). Exercises the surface the API-spec adapter claims:
# a JSON Schema (`$schema`, `$ref`, `$defs`), OpenAPI 3.x, Swagger 2.0 and
# AsyncAPI documents — the recorded dialect, the version string, ordered objects
# and the `$ref` targets. Controls pin the detection boundary (a generic JSON
# object and a JSON-Schema-shaped object with no `$schema` stay `json`; prose
# stays `opaque`).
#
#   python3 tools/fixtures/make-apispec.py --corpus DIR   # write DIR/, print TSV

import hashlib
import os
import sys


def schema():
    return (b'{"$schema":"https://json-schema.org/draft/2020-12/schema",'
            b'"type":"object","properties":{"name":{"type":"string"},'
            b'"age":{"type":"integer"}},"required":["name"],"enum":["a","b"],'
            b'"definitions":{"Legacy":{"$ref":"#/$defs/Id"}},'
            b'"$defs":{"Id":{"type":"string","format":"uuid"}}}\n')


def openapi():
    return (b'{"openapi":"3.1.0","info":{"title":"Demo","version":"1.0.0"},'
            b'"paths":{"/pets":{"get":{"responses":{"200":{"description":"ok"}}}}},'
            b'"components":{"schemas":{"Pet":{"$ref":"#/components/schemas/Pet"}}}}\n')


def swagger():
    return (b'{"swagger":"2.0","info":{"title":"Demo","version":"1.0.0"},'
            b'"basePath":"/v1","paths":{"/pets":{"get":{"responses":'
            b'{"200":{"description":"ok"}}}}},'
            b'"definitions":{"Pet":{"type":"object"}}}\n')


def asyncapi():
    return (b'{"asyncapi":"2.6.0","info":{"title":"Demo","version":"1.0.0"},'
            b'"channels":{"user/signedup":{"subscribe":{}}},'
            b'"operations":{"sendUser":{"action":"send"}},'
            b'"servers":{"prod":{"url":"broker.example.com"}}}\n')


def large():
    out = [b'{"openapi":"3.1.0","info":{"title":"Demo","version":"1.0.0"},'
           b'"paths":{']
    size = len(out[0])
    i = 0
    while size < 256 * 1024:
        item = ('"/p%06d":{"get":{"responses":{"200":{"description":"ok%d"}}}},'
                % (i, i)).encode("utf-8")
        out.append(item)
        size += len(item)
        i += 1
    out.append(b'"/tail":{"get":{"responses":{"200":{"description":"z"}}}}}}\n')
    return b"".join(out)


# --- controls ---------------------------------------------------------------

def generic_json():
    return b'{"name":"x","version":"1.0.0"}\n'


def shape_json():
    return b'{"type":"object","properties":{"a":{"type":"string"}}}\n'


def prose():
    return (b"This is just prose text.\n"
            b"It has several lines, with punctuation,\n"
            b"but no specification structure at all.\n")


LANE = [
    ("schema.json", schema),
    ("openapi.json", openapi),
    ("swagger.json", swagger),
    ("asyncapi.json", asyncapi),
    ("large.json", large),
]

CONTROLS = [
    ("generic.json", generic_json),
    ("shape.json", shape_json),
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
    print("usage: make-apispec.py --corpus DIR", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
