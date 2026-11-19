#!/usr/bin/env python3
"""Builds crates/agent/assets/lucide.oda: the one icon set OpenDesigner bundles.

Usage: scripts/build-icon-bundle.py <unpacked lucide-static package dir> <out file>
(npm pack lucide-static, or curl the tarball from registry.npmjs.org and untar it.)

Container ("ODA1"): magic, then one zlib stream of records
    u16le name_len, u16le tags_len, u32le body_len, name, tags (comma separated), body
Every Lucide icon shares one <svg> wrapper (24x24, stroke currentColor, width 2, round caps),
so only the inner shapes are stored; the app puts the wrapper back.
"""
import json, os, re, struct, sys, zlib

src, out = sys.argv[1], sys.argv[2]
tags = json.load(open(os.path.join(src, "tags.json")))
records = []
for fn in sorted(os.listdir(os.path.join(src, "icons"))):
    if not fn.endswith(".svg"):
        continue
    name = fn[:-4]
    text = open(os.path.join(src, "icons", fn), encoding="utf-8").read()
    m = re.search(r"<svg[^>]*>(.*)</svg>", text, re.S)
    body = re.sub(r">\s+<", "><", m.group(1).strip())
    body = re.sub(r"\s+", " ", body).replace(" />", "/>")
    t = ",".join(tags.get(name, [])[:10])
    records.append((name.encode(), t.encode(), body.encode()))
raw = b"".join(struct.pack("<HHI", len(n), len(t), len(b)) + n + t + b for n, t, b in records)
open(out, "wb").write(b"ODA1" + zlib.compress(raw, 9))
print(len(records), "icons", len(raw), "raw bytes", os.path.getsize(out), "compressed")
