#!/usr/bin/env python3
# Phase 21.6.2 (FIX 1) — the span-preserving conventional YAML baseline.
#
# This is the strongest realistic competitor the external review asked for: a
# **pure-Python (stdlib only)** hand-written YAML scanner that RETAINS the source
# bytes and records every node's exact byte span plus its representation
# properties. Unlike the SQLite lane it is span- *and* representation-preserving,
# so it can answer, for the same Q1–Q8 envelope grammar:
#
#   * Q1 the decoded scalar (or canonical subtree) at a dotted path;
#   * Q2 a node's exact source byte span `[start, end)`;
#   * Q3 an anchor's name and how many aliases target it (graph never expanded);
#   * Q4 a node's literal tag text (`!!str`, `!<…>`, `!custom`), or null;
#   * Q5 the document count;
#   * Q6 a scalar's style (plain/single/double/literal/folded) or container style;
#   * Q7 a `<<` merge member (surfaced, never merged);
#   * Q8 the retained source length + SHA-256 (byte-authority).
#
# The scanner is a bounded port of the *conventional* YAML 1.2 core subset the
# review asks a serious competitor to expose (the same subset `src/adapter/yaml.rs`
# supports, so that "representation preservation" is a fair fight): block/flow
# mappings and sequences, all five scalar styles, anchors/aliases, tags, comments,
# and multiple documents. It answers the SAME questions the SQLite lane answers,
# so the court compares like with like. `build` parses and writes `<db>/raw.bin`
# (the retained source) and `<db>/model.json` (the span table); `query`/`session`/
# `materialize` read them. Everything is stdlib (`json` only for serializing the
# span table and decoding a double-quoted escape).
#
#   build       --source FILE --db DIR
#   query       --db DIR --q Qn --plan JSON --out FILE
#   session     --db DIR --queries Q1,Q2,... --plan JSON --out FILE
#   materialize --db DIR --out FILE

import argparse
import hashlib
import json
import os
import sys
import time

# Kind tags mirror `src/adapter/yaml.rs`.
K_MAP, K_SEQ, K_SCALAR, K_ALIAS, K_EMPTY = range(5)
# Scalar styles.
S_PLAIN, S_SINGLE, S_DOUBLE, S_LITERAL, S_FOLDED, S_NONE = 0, 1, 2, 3, 4, 255
# Container styles.
C_BLOCK, C_FLOW = 0, 1

KIND_NAME = {
    K_MAP: "mapping", K_SEQ: "sequence", K_SCALAR: "scalar",
    K_ALIAS: "alias", K_EMPTY: "null",
}


def kind_name(k):
    return KIND_NAME.get(k, "unknown")


def style_name(kind, style):
    if kind in (K_MAP, K_SEQ):
        return {C_BLOCK: "block", C_FLOW: "flow"}.get(style, "none")
    if kind == K_SCALAR:
        return {S_PLAIN: "plain", S_SINGLE: "single", S_DOUBLE: "double",
                S_LITERAL: "literal", S_FOLDED: "folded"}.get(style, "none")
    return "none"


def sha256_hex(b):
    return hashlib.sha256(b).hexdigest()


def now_us():
    return int(time.monotonic() * 1_000_000)


def envelope(q, value=None, *, declined=False, code=None, reason="", detail=None):
    e = {"q": q, "lane": "spanpy", "declined": bool(declined), "native": True,
         "value": None if declined else value, "detail": detail or {}}
    e["decline"] = {"code": code, "detail": reason} if declined else None
    return e


class YamlError(Exception):
    """A typed parse/resolve decline (never a crash)."""

    def __init__(self, code, msg):
        super().__init__(msg)
        self.code = code
        self.msg = msg


def _corrupt(msg):
    return YamlError("malformed", msg)


def _unsupported(msg):
    return YamlError("unsupported", msg)


def _usage(msg):
    return YamlError("usage", msg)


# --- the bounded scanner -----------------------------------------------------
#
# A node is a list: [kind, style, anchor, tag, alias, start, end, children].


class Scanner:
    def __init__(self, src):
        self.b = src
        self.n = len(src)
        self.at = 0
        self.nodes = []
        self.docs = []          # [root, root_kind, start, end, explicit]
        self.comments = []
        self.last_end = 0

    # -- cursor helpers -------------------------------------------------------

    def peek(self):
        return self.b[self.at] if self.at < self.n else None

    def skip_spaces(self):
        b, n = self.b, self.n
        while self.at < n and b[self.at] in (0x20, 0x09):
            self.at += 1

    def line_start(self):
        i = self.at
        b = self.b
        while i > 0 and b[i - 1] not in (0x0A, 0x0D):
            i -= 1
        return i

    def col(self):
        return self.at - self.line_start()

    def line_info_at(self, start):
        b, n = self.b, self.n
        i = start
        indent = 0
        tab = False
        while i < n and b[i] in (0x20, 0x09):
            if b[i] == 0x09:
                tab = True
            indent += 1
            i += 1
        content = i
        j = i
        while j < n and b[j] not in (0x0A, 0x0D):
            j += 1
        eol = j
        end = j
        if end < n and b[end] == 0x0D:
            end += 1
        if end < n and b[end] == 0x0A:
            end += 1
        return {"indent": indent, "tab": tab, "blank": j == content,
                "content": content, "eol": eol, "end": end}

    def at_line_marker(self, m):
        if self.col() != 0:
            return False
        end = self.at + len(m)
        if end > self.n or self.b[self.at:end] != m:
            return False
        nxt = self.b[end] if end < self.n else None
        return nxt in (None, 0x20, 0x09, 0x0A, 0x0D, 0x23)

    def consume_newline(self):
        c = self.peek()
        if c == 0x0A:
            self.at += 1
            return
        if c == 0x0D:
            self.at += 1
            if self.peek() == 0x0A:
                self.at += 1
            return
        raise _corrupt("expected end of line")

    def consume_line_end(self):
        self.skip_spaces()
        if self.peek() == 0x23:  # '#'
            s = self.at
            while self.peek() is not None and self.peek() not in (0x0A, 0x0D):
                self.at += 1
            self.comments.append((s, self.at))
        c = self.peek()
        if c is None:
            return
        if c in (0x0A, 0x0D):
            self.consume_newline()
            return
        raise _corrupt("unexpected content after the node on this line")

    def end_entry(self):
        self.skip_spaces()
        c = self.peek()
        if c is None or c in (0x23, 0x0A, 0x0D):
            self.consume_line_end()

    def skip_blank_comment_lines(self):
        while self.at < self.n:
            li = self.line_info_at(self.at)
            if li["blank"]:
                self.at = li["end"]
                continue
            if self.b[li["content"]] == 0x23:
                self.comments.append((li["content"], li["eol"]))
                self.at = li["end"]
                continue
            if li["tab"]:
                raise _corrupt("tab used for indentation")
            self.at = li["content"]
            break

    # -- nodes ----------------------------------------------------------------

    def new_node(self, kind, style, start):
        self.nodes.append([kind, style, None, None, None, start, start, []])
        return len(self.nodes) - 1

    def finish_node(self, idx, end, children):
        self.last_end = end
        n = self.nodes[idx]
        n[6] = end
        n[7] = children

    def empty_node(self, pos):
        idx = self.new_node(K_EMPTY, S_NONE, pos)
        self.finish_node(idx, pos, [])
        return idx

    # -- stream ---------------------------------------------------------------

    def parse_stream(self):
        self.skip_blank_comment_lines()
        if self.at >= self.n:
            return
        if self.peek() == 0x25:  # '%'
            raise _unsupported("YAML directives are not supported")
        while True:
            if self.at_line_marker(b"---"):
                doc_start = self.at
                explicit = True
                self.at += 3
                self.consume_line_end()
                self.skip_blank_comment_lines()
            elif self.at_line_marker(b"..."):
                raise _corrupt("document end marker without a start")
            else:
                doc_start = self.at
                explicit = False
            if (self.at >= self.n or self.at_line_marker(b"---")
                    or self.at_line_marker(b"...")):
                root, kind = self.empty_node(self.at), K_EMPTY
            else:
                root, kind = self.parse_block_node(1)
            doc_end = self.last_end
            self.docs.append([root, kind, doc_start, doc_end, explicit])
            self.end_entry()
            self.skip_blank_comment_lines()
            if self.at >= self.n:
                break
            if self.at_line_marker(b"..."):
                self.at += 3
                self.consume_line_end()
                self.skip_blank_comment_lines()
                if self.at >= self.n:
                    break
                if not self.at_line_marker(b"---"):
                    raise _corrupt("content after a document end marker")
                continue
            if self.at_line_marker(b"---"):
                continue
            raise _corrupt("unexpected trailing content after a YAML document")

    # -- properties -----------------------------------------------------------

    def parse_properties(self):
        anchor = None
        tag = None
        while True:
            c = self.peek()
            if c == 0x26:  # '&'
                if anchor is not None:
                    raise _corrupt("duplicate anchor property")
                self.at += 1
                anchor = self.scan_name()
                self.skip_spaces()
            elif c == 0x21:  # '!'
                if tag is not None:
                    raise _corrupt("duplicate tag property")
                tag = self.scan_tag()
                self.skip_spaces()
            else:
                break
        return anchor, tag

    def scan_name(self):
        start = self.at
        b, n = self.b, self.n
        while self.at < n:
            c = b[self.at]
            if c in (0x20, 0x09, 0x0A, 0x0D, 0x2C, 0x5B, 0x5D, 0x7B, 0x7D, 0x23):
                break
            self.at += 1
        if self.at == start:
            raise _corrupt("empty anchor/alias name")
        return self.b[start:self.at].decode("utf-8", "strict")

    def scan_tag(self):
        start = self.at
        self.at += 1  # '!'
        if self.peek() == 0x3C:  # '<'
            self.at += 1
            while True:
                c = self.peek()
                if c == 0x3E:  # '>'
                    self.at += 1
                    break
                if c is None or c in (0x0A, 0x0D):
                    raise _corrupt("unterminated verbatim tag")
                self.at += 1
        else:
            while self.peek() is not None:
                c = self.peek()
                if c in (0x20, 0x09, 0x0A, 0x0D, 0x2C, 0x5B, 0x5D, 0x7B, 0x7D):
                    break
                self.at += 1
        return self.b[start:self.at].decode("utf-8", "strict")

    # -- block nodes ----------------------------------------------------------

    def at_dash(self):
        return self.peek() == 0x2D and (
            self.b[self.at + 1] if self.at + 1 < self.n else None
        ) in (None, 0x20, 0x09, 0x0A, 0x0D)

    def followed_by_space_or_eol(self):
        return (self.b[self.at + 1] if self.at + 1 < self.n else None) in (
            None, 0x20, 0x09, 0x0A, 0x0D)

    def parse_block_node(self, depth):
        if depth > 512:
            raise _unsupported("nesting too deep")
        self.skip_spaces()
        anchor, tag = self.parse_properties()
        self.skip_spaces()
        c = self.peek()
        if c is None:
            raise _corrupt("unexpected end of input")
        if c == 0x2D and self.at_dash():
            idx, kind = self.parse_block_sequence(depth)
        elif c == 0x3F and self.followed_by_space_or_eol():
            raise _unsupported("explicit keys are not supported")
        elif self.find_key_colon() is not None:
            idx, kind = self.parse_block_mapping(depth)
        else:
            idx, kind = self.inline_value(0, depth)
        n = self.nodes[idx]
        n[2] = anchor
        n[3] = tag
        return idx, kind

    def parse_block_sequence(self, depth):
        indent = self.col()
        start = self.at
        idx = self.new_node(K_SEQ, C_BLOCK, start)
        children = []
        last = start
        while True:
            col = self.col()
            if col < indent:
                break
            if col > indent:
                raise _corrupt("bad indentation in a block sequence")
            if not self.at_dash():
                break
            self.at += 1
            elem = self.parse_seq_element(indent, depth + 1)
            children.append(elem)
            last = self.last_end
            self.end_entry()
            self.skip_blank_comment_lines()
            if (self.at >= self.n or self.at_line_marker(b"---")
                    or self.at_line_marker(b"...")):
                break
        self.finish_node(idx, last, children)
        return idx, K_SEQ

    def parse_seq_element(self, seq_indent, depth):
        self.skip_spaces()
        c = self.peek()
        if c is None or c == 0x23:
            return self.empty_node(self.at)
        if c in (0x0A, 0x0D):
            self.consume_newline()
            self.skip_blank_comment_lines()
            if (self.at >= self.n or self.at_line_marker(b"---")
                    or self.at_line_marker(b"...")):
                return self.empty_node(self.at)
            if self.col() > seq_indent:
                return self.parse_block_node(depth)[0]
            return self.empty_node(self.at)
        anchor, tag = self.parse_properties()
        self.skip_spaces()
        c = self.peek()
        if c is None:
            raise _corrupt("unexpected end of input")
        if c == 0x2D and self.at_dash():
            idx = self.parse_block_sequence(depth)[0]
        elif self.find_key_colon() is not None:
            idx = self.parse_block_mapping(depth)[0]
        else:
            idx = self.inline_value(seq_indent, depth)[0]
        n = self.nodes[idx]
        n[2] = anchor
        n[3] = tag
        return idx

    def parse_block_mapping(self, depth):
        indent = self.col()
        start = self.at
        idx = self.new_node(K_MAP, C_BLOCK, start)
        children = []
        last = start
        while True:
            col = self.col()
            if col < indent:
                break
            if col > indent:
                raise _corrupt("bad indentation in a block mapping")
            key = self.parse_key_node(depth)
            self.skip_spaces()
            if self.peek() != 0x3A:  # ':'
                raise _corrupt("expected ':' after a mapping key")
            self.at += 1
            val = self.parse_block_value(indent, depth + 1)
            children.append(key)
            children.append(val)
            last = self.last_end
            self.end_entry()
            self.skip_blank_comment_lines()
            if (self.at >= self.n or self.at_line_marker(b"---")
                    or self.at_line_marker(b"...")):
                break
        self.finish_node(idx, last, children)
        return idx, K_MAP

    def parse_block_value(self, key_indent, depth):
        self.skip_spaces()
        props_start = self.at
        anchor, tag = self.parse_properties()
        self.skip_spaces()
        c = self.peek()
        if c is None or c == 0x23:
            idx = self.empty_node(props_start)
            n = self.nodes[idx]
            n[2] = anchor
            n[3] = tag
            return idx
        if c in (0x0A, 0x0D):
            self.consume_newline()
            self.skip_blank_comment_lines()
            empty = (self.at >= self.n or self.at_line_marker(b"---")
                     or self.at_line_marker(b"..."))
            if empty:
                idx = self.empty_node(props_start)
            else:
                col = self.col()
                if col > key_indent:
                    idx = self.parse_block_node(depth)[0]
                elif col == key_indent and self.at_dash():
                    idx = self.parse_block_sequence(depth)[0]
                else:
                    idx = self.empty_node(props_start)
            n = self.nodes[idx]
            n[2] = anchor
            n[3] = tag
            return idx
        idx = self.inline_value(key_indent, depth)[0]
        n = self.nodes[idx]
        n[2] = anchor
        n[3] = tag
        return idx

    def inline_value(self, parent_indent, depth):
        c = self.peek()
        if c is None:
            raise _corrupt("unexpected end of input")
        if c == 0x2A:  # '*'
            return self.parse_alias()
        if c == 0x5B:  # '['
            return self.parse_flow_seq(depth)
        if c == 0x7B:  # '{'
            return self.parse_flow_map(depth)
        if c == 0x27:  # '
            return self.parse_single_quoted()
        if c == 0x22:  # "
            return self.parse_double_quoted()
        if c in (0x7C, 0x3E):  # '|' '>'
            return self.parse_block_scalar(parent_indent, depth)
        return self.parse_plain_scalar_block()

    def parse_key_node(self, depth):
        c = self.peek()
        if c is None:
            raise _corrupt("expected a mapping key")
        if c == 0x27:
            return self.parse_single_quoted()[0]
        if c == 0x22:
            return self.parse_double_quoted()[0]
        if c in (0x5B, 0x7B):
            raise _unsupported("flow-collection keys are not supported")
        if c in (0x26, 0x21, 0x2A):
            raise _unsupported("anchored/tagged/aliased keys are not supported")
        if c == 0x3F and self.followed_by_space_or_eol():
            raise _unsupported("explicit keys are not supported")
        return self.parse_plain_key()

    def parse_plain_key(self):
        start = self.at
        colon = self.find_key_colon()
        if colon is None:
            raise _corrupt("expected ':' after a mapping key")
        e = colon
        while e > start and self.b[e - 1] in (0x20, 0x09):
            e -= 1
        if e == start:
            raise _corrupt("empty mapping key")
        idx = self.new_node(K_SCALAR, S_PLAIN, start)
        self.at = e
        self.finish_node(idx, e, [])
        return idx

    def find_key_colon(self):
        b, n = self.b, self.n
        i = self.at
        if i >= n:
            return None
        if b[i] in (0x27, 0x22):
            i = self.skip_quoted_at(i)
            if i is None:
                return None
            while i < n and b[i] in (0x20, 0x09):
                i += 1
            if i < n and b[i] == 0x3A and self.colon_ok(i):
                return i
            return None
        while i < n:
            c = b[i]
            if c in (0x0A, 0x0D):
                return None
            if c == 0x3A and self.colon_ok(i):
                return i
            if c == 0x23 and i > self.at and b[i - 1] in (0x20, 0x09):
                return None
            i += 1
        return None

    def colon_ok(self, i):
        return (self.b[i + 1] if i + 1 < self.n else None) in (
            None, 0x20, 0x09, 0x0A, 0x0D)

    def skip_quoted_at(self, start):
        b, n = self.b, self.n
        quote = b[start]
        i = start + 1
        while i < n:
            c = b[i]
            if c == quote:
                if quote == 0x27 and i + 1 < n and b[i + 1] == 0x27:
                    i += 2
                    continue
                return i + 1
            if c == 0x5C and quote == 0x22:
                i += 2
                continue
            if c in (0x0A, 0x0D):
                return None
            i += 1
        return None

    # -- scalars --------------------------------------------------------------

    def parse_single_quoted(self):
        start = self.at
        self.at += 1
        while True:
            c = self.peek()
            if c is None:
                raise _corrupt("unterminated single-quoted scalar")
            if c == 0x27:
                if self.at + 1 < self.n and self.b[self.at + 1] == 0x27:
                    self.at += 2
                    continue
                self.at += 1
                break
            if c in (0x0A, 0x0D):
                raise _corrupt("a single-quoted scalar may not span lines")
            self.at += 1
        idx = self.new_node(K_SCALAR, S_SINGLE, start)
        self.finish_node(idx, self.at, [])
        return idx, K_SCALAR

    def parse_double_quoted(self):
        start = self.at
        self.at += 1
        while True:
            c = self.peek()
            if c is None:
                raise _corrupt("unterminated double-quoted scalar")
            if c == 0x22:
                self.at += 1
                break
            if c == 0x5C:
                self.at += 1
                if self.peek() in (None, 0x0A, 0x0D):
                    raise _corrupt("a double-quoted scalar may not span lines")
                self.at += 1
                continue
            if c in (0x0A, 0x0D):
                raise _corrupt("a double-quoted scalar may not span lines")
            self.at += 1
        idx = self.new_node(K_SCALAR, S_DOUBLE, start)
        self.finish_node(idx, self.at, [])
        return idx, K_SCALAR

    def parse_plain_scalar_block(self):
        start = self.at
        end = start
        b, n = self.b, self.n
        while self.at < n:
            c = b[self.at]
            if c in (0x0A, 0x0D):
                break
            if c == 0x23 and end > start and b[end - 1] in (0x20, 0x09):
                break
            if c == 0x3A and self.colon_ok(self.at):
                raise _corrupt("unexpected ':' in a plain scalar value")
            self.at += 1
            end = self.at
        e = end
        while e > start and b[e - 1] in (0x20, 0x09):
            e -= 1
        if e == start:
            raise _corrupt("empty plain scalar")
        idx = self.new_node(K_SCALAR, S_PLAIN, start)
        self.finish_node(idx, e, [])
        return idx, K_SCALAR

    def parse_alias(self):
        start = self.at
        self.at += 1
        name = self.scan_name()
        idx = self.new_node(K_ALIAS, S_NONE, start)
        self.nodes[idx][4] = name
        self.finish_node(idx, self.at, [])
        return idx, K_ALIAS

    def parse_block_scalar(self, parent_indent, depth):
        start = self.at
        style = S_LITERAL if self.peek() == 0x7C else S_FOLDED
        self.at += 1
        explicit = None
        while True:
            c = self.peek()
            if c in (0x2B, 0x2D):  # '+' '-'
                self.at += 1
            elif c is not None and 0x31 <= c <= 0x39:  # '1'..'9'
                explicit = c - 0x30
                self.at += 1
            else:
                break
        self.skip_spaces()
        if self.peek() == 0x23:
            s = self.at
            while self.peek() is not None and self.peek() not in (0x0A, 0x0D):
                self.at += 1
            self.comments.append((s, self.at))
        c = self.peek()
        if c in (0x0A, 0x0D):
            self.consume_newline()
        elif c is not None:
            raise _corrupt("unexpected content after a block scalar header")
        if explicit is not None:
            content_indent = parent_indent + explicit
        else:
            p = self.at
            ci = parent_indent + 1
            while p < self.n:
                li = self.line_info_at(p)
                if li["blank"]:
                    p = li["end"]
                    continue
                if li["tab"]:
                    raise _corrupt("tab in block scalar indentation")
                if li["indent"] <= parent_indent:
                    break
                ci = li["indent"]
                break
            content_indent = ci
        end = self.at
        while end < self.n:
            li = self.line_info_at(end)
            if li["blank"]:
                end = li["end"]
                continue
            if li["tab"]:
                raise _corrupt("tab in block scalar indentation")
            if li["indent"] >= content_indent:
                end = li["end"]
                continue
            break
        self.at = end
        idx = self.new_node(K_SCALAR, style, start)
        self.finish_node(idx, end, [])
        return idx, K_SCALAR

    # -- flow -----------------------------------------------------------------

    def skip_flow_ws(self):
        while True:
            c = self.peek()
            if c in (0x20, 0x09, 0x0A, 0x0D):
                self.at += 1
            elif c == 0x23:
                s = self.at
                while self.peek() is not None and self.peek() not in (0x0A, 0x0D):
                    self.at += 1
                self.comments.append((s, self.at))
            else:
                break

    def parse_flow_node(self, depth):
        self.skip_flow_ws()
        anchor, tag = self.parse_properties()
        self.skip_flow_ws()
        c = self.peek()
        if c is None:
            raise _corrupt("unexpected end of a flow collection")
        if c == 0x2A:
            idx = self.parse_alias()[0]
        elif c == 0x5B:
            idx = self.parse_flow_seq(depth)[0]
        elif c == 0x7B:
            idx = self.parse_flow_map(depth)[0]
        elif c == 0x27:
            idx = self.parse_single_quoted()[0]
        elif c == 0x22:
            idx = self.parse_double_quoted()[0]
        elif c in (0x7C, 0x3E):
            raise _unsupported("a block scalar is not allowed in flow context")
        elif c == 0x3F and self.followed_by_space_or_eol():
            raise _unsupported("explicit keys are not supported")
        elif c == 0x3A:
            raise _corrupt("unexpected ':' in a flow node")
        else:
            idx = self.parse_plain_scalar_flow()[0]
        n = self.nodes[idx]
        n[2] = anchor
        n[3] = tag
        return idx

    def parse_flow_seq(self, depth):
        start = self.at
        self.at += 1
        idx = self.new_node(K_SEQ, C_FLOW, start)
        children = []
        self.skip_flow_ws()
        if self.peek() == 0x5D:
            self.at += 1
            self.finish_node(idx, self.at, children)
            return idx, K_SEQ
        while True:
            elem = self.parse_flow_node(depth + 1)
            children.append(elem)
            self.skip_flow_ws()
            c = self.peek()
            if c == 0x2C:
                self.at += 1
                self.skip_flow_ws()
                if self.peek() == 0x5D:
                    self.at += 1
                    break
            elif c == 0x5D:
                self.at += 1
                break
            else:
                raise _corrupt("expected ',' or ']' in a flow sequence")
        self.finish_node(idx, self.at, children)
        return idx, K_SEQ

    def parse_flow_map(self, depth):
        start = self.at
        self.at += 1
        idx = self.new_node(K_MAP, C_FLOW, start)
        children = []
        self.skip_flow_ws()
        if self.peek() == 0x7D:
            self.at += 1
            self.finish_node(idx, self.at, children)
            return idx, K_MAP
        while True:
            key = self.parse_flow_node(depth + 1)
            self.skip_flow_ws()
            if self.peek() != 0x3A:
                raise _corrupt("expected ':' in a flow mapping")
            self.at += 1
            self.skip_flow_ws()
            if self.peek() in (0x2C, 0x7D):
                val = self.empty_node(self.at)
            else:
                val = self.parse_flow_node(depth + 1)
            children.append(key)
            children.append(val)
            self.skip_flow_ws()
            c = self.peek()
            if c == 0x2C:
                self.at += 1
                self.skip_flow_ws()
                if self.peek() == 0x7D:
                    self.at += 1
                    break
            elif c == 0x7D:
                self.at += 1
                break
            else:
                raise _corrupt("expected ',' or '}' in a flow mapping")
        self.finish_node(idx, self.at, children)
        return idx, K_MAP

    def parse_plain_scalar_flow(self):
        c = self.peek()
        if c in (0x3A, 0x2C, 0x5B, 0x5D, 0x7B, 0x7D):
            raise _corrupt("empty plain scalar in flow")
        start = self.at
        end = start
        b, n = self.b, self.n
        while self.at < n:
            c = b[self.at]
            if c in (0x2C, 0x5B, 0x5D, 0x7B, 0x7D, 0x0A, 0x0D):
                break
            if c == 0x23 and end > start and b[end - 1] in (0x20, 0x09):
                break
            if c == 0x3A and (self.b[self.at + 1] if self.at + 1 < n else None) in (
                    None, 0x20, 0x09, 0x2C, 0x5D, 0x7D):
                break
            self.at += 1
            end = self.at
        e = end
        while e > start and b[e - 1] in (0x20, 0x09):
            e -= 1
        if e == start:
            raise _corrupt("empty plain scalar in flow")
        idx = self.new_node(K_SCALAR, S_PLAIN, start)
        self.finish_node(idx, e, [])
        return idx, K_SCALAR


# --- model helpers -----------------------------------------------------------


def token_bytes(raw, node):
    return raw[node[5]:node[6]]


def _decode_single(tok):
    if len(tok) < 2 or tok[0] != 0x27 or tok[-1] != 0x27:
        raise _corrupt("not a well-formed single-quoted scalar")
    inner = tok[1:-1]
    out = bytearray()
    i = 0
    while i < len(inner):
        if inner[i] == 0x27:
            if i + 1 < len(inner) and inner[i + 1] == 0x27:
                out.append(0x27)
                i += 2
                continue
            raise _corrupt("stray quote in single-quoted scalar")
        out.append(inner[i])
        i += 1
    return out.decode("utf-8", "strict")


_DOUBLE_ESC = {
    0x30: b"\x00", 0x61: b"\x07", 0x62: b"\x08", 0x74: b"\x09", 0x6E: b"\x0a",
    0x76: b"\x0b", 0x66: b"\x0c", 0x72: b"\x0d", 0x65: b"\x1b", 0x20: b" ",
    0x22: b'"', 0x2F: b"/", 0x5C: b"\\",
}


def _decode_double(tok):
    if len(tok) < 2 or tok[0] != 0x22 or tok[-1] != 0x22:
        raise _corrupt("not a well-formed double-quoted scalar")
    inner = tok[1:-1]
    out = bytearray()
    i = 0
    while i < len(inner):
        c = inner[i]
        if c != 0x5C:
            out.append(c)
            i += 1
            continue
        i += 1
        if i >= len(inner):
            raise _corrupt("scalar ends inside an escape")
        e = inner[i]
        i += 1
        if e in _DOUBLE_ESC:
            out.extend(_DOUBLE_ESC[e])
        elif e == 0x4E:
            out.extend("\u0085".encode("utf-8"))
        elif e == 0x5F:
            out.extend("\u00a0".encode("utf-8"))
        elif e == 0x4C:
            out.extend("\u2028".encode("utf-8"))
        elif e == 0x50:
            out.extend("\u2029".encode("utf-8"))
        elif e in (0x78, 0x75, 0x55):
            width = {0x78: 2, 0x75: 4, 0x55: 8}[e]
            if i + width > len(inner):
                raise _corrupt("truncated hex escape")
            v = int(inner[i:i + width].decode("ascii"), 16)
            i += width
            out.extend(chr(v).encode("utf-8"))
        else:
            raise _corrupt("invalid escape in double-quoted scalar")
    return out.decode("utf-8", "strict")


def _decode_block(tok, folded):
    nl = tok.find(b"\n")
    if nl < 0:
        raise _corrupt("block scalar has no header line")
    body = tok[nl + 1:]
    lines = body.split(b"\n")
    if lines and lines[-1] == b"":
        lines.pop()
    stripped_indent = None
    for l in lines:
        if all(c in (0x20, 0x09) for c in l):
            continue
        n = 0
        while n < len(l) and l[n] == 0x20:
            n += 1
        if stripped_indent is None or n < stripped_indent:
            stripped_indent = n
    if stripped_indent is None:
        stripped_indent = 0
    stripped = []
    for l in lines:
        n = 0
        while n < len(l) and l[n] == 0x20:
            n += 1
        stripped.append(l[min(n, stripped_indent):])
    out = bytearray()
    if folded:
        prev_nonblank = False
        for l in stripped:
            blank = len(l) == 0
            if not blank and prev_nonblank:
                out.extend(b" ")
            out.extend(l)
            out.extend(b"\n")
            prev_nonblank = not blank
    else:
        for l in stripped:
            out.extend(l)
            out.extend(b"\n")
    return out.decode("utf-8", "strict")


def decode_scalar_value(raw, node):
    tok = token_bytes(raw, node)
    style = node[1]
    if style == S_PLAIN:
        return tok.decode("utf-8", "replace")
    if style == S_SINGLE:
        return _decode_single(tok)
    if style == S_DOUBLE:
        return _decode_double(tok)
    if style == S_LITERAL:
        return _decode_block(tok, False)
    if style == S_FOLDED:
        return _decode_block(tok, True)
    return tok.decode("utf-8", "replace")


def render(model, raw, index, out):
    node = model[index]
    kind = node[0]
    if kind == K_MAP:
        out.append("{")
        children = node[7]
        i = 0
        while i + 1 < len(children):
            if i > 0:
                out.append(",")
            render(model, raw, children[i], out)
            out.append(":")
            render(model, raw, children[i + 1], out)
            i += 2
        out.append("}")
    elif kind == K_SEQ:
        out.append("[")
        for i, c in enumerate(node[7]):
            if i > 0:
                out.append(",")
            render(model, raw, c, out)
        out.append("]")
    elif kind == K_ALIAS:
        out.append("*")
        out.append(node[4] or "")
    elif kind == K_EMPTY:
        out.append("null")
    else:
        out.append(token_bytes(raw, node).decode("utf-8", "replace"))


def subtree_text(model, raw, index):
    out = []
    render(model, raw, index, out)
    return "".join(out)


def node_text(model, raw, index):
    node = model[index]
    kind = node[0]
    if kind == K_SCALAR:
        return decode_scalar_value(raw, node)
    if kind in (K_MAP, K_SEQ):
        return subtree_text(model, raw, index)
    if kind == K_ALIAS:
        return "*" + (node[4] or "")
    return "null"


def resolve_anchor(model, name):
    for i, n in enumerate(model):
        if n[2] == name:
            return i
    return None


def parse_index(seg):
    if seg == "" or (len(seg) > 1 and seg[0] == "0"):
        raise _usage("sequence segment %r is not a canonical index" % seg)
    if not seg.isdigit():
        raise _usage("sequence index %r is not a non-negative integer" % seg)
    return int(seg)


def resolve_path(model, raw, path, docs):
    parts = path.split(".") if path else []
    doc = 0
    if parts:
        first = parts[0]
        if first.startswith("doc") and first[3:].isdigit():
            doc = int(first[3:])
            parts = parts[1:]
    if doc >= len(docs):
        raise _unsupported("YAML stream has no document %d" % doc)
    index = docs[doc][0]
    matches = 1
    for seg in parts:
        hops = 0
        while model[index][0] == K_ALIAS:
            name = model[index][4]
            target = resolve_anchor(model, name)
            if target is None:
                raise _unsupported("YAML alias targets unknown anchor %r" % name)
            index = target
            hops += 1
            if hops > len(model):
                raise _corrupt("alias cycle while resolving a path")
        node = model[index]
        kind = node[0]
        if kind == K_MAP:
            found = 0
            first = None
            children = node[7]
            i = 0
            while i + 1 < len(children):
                key_idx = children[i]
                val_idx = children[i + 1]
                i += 2
                if model[key_idx][0] == K_SCALAR and \
                        decode_scalar_value(raw, model[key_idx]) == seg:
                    found += 1
                    if first is None:
                        first = val_idx
            if first is None:
                raise _unsupported("YAML mapping has no key %r" % seg)
            index = first
            matches = found
        elif kind == K_SEQ:
            k = parse_index(seg)
            if k >= len(node[7]):
                raise _unsupported("YAML sequence index %d out of range" % k)
            index = node[7][k]
            matches = 1
        else:
            raise _unsupported("cannot descend into a YAML %s at %r" % (kind_name(kind), seg))
    return index, matches


# --- queries -----------------------------------------------------------------


def _load(db_dir):
    with open(os.path.join(db_dir, "raw.bin"), "rb") as f:
        raw = f.read()
    with open(os.path.join(db_dir, "model.json")) as f:
        model = json.load(f)
    docs_meta = json.load(open(os.path.join(db_dir, "docs.json")))
    return raw, model, docs_meta


def run_query(raw, model, docs, q, plan):
    def resolve(p):
        return resolve_path(model, raw, p, docs)

    if q == "Q1":
        try:
            index, _ = resolve(plan["value_path"])
        except YamlError as e:
            return envelope("Q1", declined=True, code=e.code, reason=e.msg)
        return envelope("Q1", node_text(model, raw, index))
    if q == "Q2":
        try:
            index, _ = resolve(plan["span_path"])
        except YamlError as e:
            return envelope("Q2", declined=True, code=e.code, reason=e.msg)
        node = model[index]
        return envelope("Q2", [node[5], node[6]])
    if q == "Q3":
        name = plan["anchor"]
        target = resolve_anchor(model, name)
        if target is None:
            return envelope("Q3", declined=True, code="no-anchor",
                            reason="no node declares anchor %r" % name)
        alias_count = sum(1 for n in model if n[0] == K_ALIAS and n[4] == name)
        return envelope("Q3", {"anchor": name, "alias_count": alias_count})
    if q == "Q4":
        try:
            index, _ = resolve(plan["tag_path"])
        except YamlError as e:
            return envelope("Q4", declined=True, code=e.code, reason=e.msg)
        return envelope("Q4", model[index][3])
    if q == "Q5":
        return envelope("Q5", len(docs))
    if q == "Q6":
        try:
            index, _ = resolve(plan["style_path"])
        except YamlError as e:
            return envelope("Q6", declined=True, code=e.code, reason=e.msg)
        node = model[index]
        return envelope("Q6", style_name(node[0], node[1]))
    if q == "Q7":
        try:
            index, matches = resolve(plan["merge_path"])
        except YamlError as e:
            return envelope("Q7", declined=True, code=e.code, reason=e.msg)
        return envelope("Q7", {"kind": kind_name(model[index][0]), "matches": matches})
    if q == "Q8":
        return envelope("Q8", {"length": len(raw), "sha256": sha256_hex(raw)})
    return envelope(q, declined=True, code="unknown-question", reason=q)


# --- CLI ---------------------------------------------------------------------


def parse_source(raw):
    sc = Scanner(raw)
    sc.parse_stream()
    return sc.nodes, sc.docs


def build(source, db_dir):
    os.makedirs(db_dir, exist_ok=True)
    for name in ("raw.bin", "model.json", "meta.json", "docs.json"):
        try:
            os.remove(os.path.join(db_dir, name))
        except FileNotFoundError:
            pass
    t0 = now_us()
    with open(source, "rb") as f:
        raw = f.read()
    nodes, docs = parse_source(raw)
    extract_us = now_us() - t0
    with open(os.path.join(db_dir, "raw.bin"), "wb") as f:
        f.write(raw)
    with open(os.path.join(db_dir, "model.json"), "w") as f:
        json.dump(nodes, f, separators=(",", ":"))
    with open(os.path.join(db_dir, "docs.json"), "w") as f:
        json.dump(docs, f, separators=(",", ":"))
    with open(os.path.join(db_dir, "meta.json"), "w") as f:
        json.dump({"doc_len": len(raw), "nodes": len(nodes),
                   "docs": len(docs)}, f, sort_keys=True)
    print(json.dumps({"ok": True, "fmt": "yaml", "src_len": len(raw),
                      "nodes": len(nodes), "docs": len(docs),
                      "extract_us": extract_us}, sort_keys=True))
    return 0


def query(db_dir, q, plan, out):
    raw, model, docs = _load(db_dir)
    env = run_query(raw, model, docs, q, plan)
    env["q"] = q
    with open(out, "w") as f:
        json.dump(env, f, sort_keys=True)
    return 0


def session(db_dir, queries, plan, out):
    raw, model, docs = _load(db_dir)
    batch = []
    for q in queries.split(","):
        q = q.strip()
        if not q:
            continue
        t0 = now_us()
        env = run_query(raw, model, docs, q, plan)
        us = now_us() - t0
        env["q"] = q
        batch.append({"q": q, "us": us, "env": env})
    with open(out, "w") as f:
        json.dump({"batch": batch}, f, sort_keys=True)
    return 0


def materialize(db_dir, out):
    with open(os.path.join(db_dir, "raw.bin"), "rb") as f:
        raw = f.read()
    with open(out, "wb") as f:
        f.write(raw)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--source", required=True)
    b.add_argument("--db", required=True)
    q = sub.add_parser("query")
    q.add_argument("--db", required=True)
    q.add_argument("--q", required=True)
    q.add_argument("--plan", default="{}")
    q.add_argument("--out", required=True)
    s = sub.add_parser("session")
    s.add_argument("--db", required=True)
    s.add_argument("--queries", required=True)
    s.add_argument("--plan", default="{}")
    s.add_argument("--out", required=True)
    m = sub.add_parser("materialize")
    m.add_argument("--db", required=True)
    m.add_argument("--out", required=True)
    ns = ap.parse_args(argv)
    if ns.cmd == "build":
        return build(ns.source, ns.db)
    if ns.cmd == "query":
        return query(ns.db, ns.q, json.loads(ns.plan), ns.out)
    if ns.cmd == "session":
        return session(ns.db, ns.queries, json.loads(ns.plan), ns.out)
    if ns.cmd == "materialize":
        return materialize(ns.db, ns.out)
    return 2


if __name__ == "__main__":
    sys.exit(main())
