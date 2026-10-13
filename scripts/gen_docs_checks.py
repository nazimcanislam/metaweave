#!/usr/bin/env python3
"""Regenerates the check catalogue inside docs/index.html from src/catalog.rs.

Usage: python3 scripts/gen_docs_checks.py
"""
import html
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
catalog = (ROOT / "src" / "catalog.rs").read_text(encoding="utf-8")
page = ROOT / "docs" / "index.html"

STR = r'"((?:[^"\\]|\\.)*)"'
ENTRY = re.compile(r"c\(\s*" + STR + r"\s*,\s*(\w+)\s*,\s*(true|false)\s*,\s*" + STR + r"\s*,\s*" + STR + r"\s*,\s*" + STR + r"\s*,?\s*\)", re.S)
unescape = lambda s: s.replace('\\"', '"').replace("\\\\", "\\")

LABELS = {
    "Seo": "SEO",
    "Social": "Social previews",
    "Accessibility": "Accessibility",
    "Performance": "Performance",
    "Security": "Security",
    "Assets": "Site files & icons",
    "Links": "Links & resources",
    "Html": "HTML hygiene",
}
groups: dict[str, list] = {}
for m in ENTRY.finditer(catalog):
    cid, cat, net, title, why, fix = m.groups()
    groups.setdefault(cat, []).append((unescape(cid), net == "true", unescape(title), unescape(why)))

total = sum(len(v) for v in groups.values())
out = []
for cat, label in LABELS.items():
    items = groups.get(cat, [])
    if not items:
        continue
    out.append(f'      <details class="cat"{" open" if cat == "Social" else ""}>')
    out.append(f'        <summary>{html.escape(label)}<span class="n">{len(items)} checks</span></summary>')
    out.append("        <dl>")
    for cid, net, title, why in items:
        out.append(f"          <dt>{html.escape(cid)}</dt>")
        out.append(f"          <dd>{html.escape(title)}<small>{html.escape(why)}</small></dd>")
    out.append("        </dl>")
    out.append("      </details>")

block = "\n".join(out)
text = page.read_text(encoding="utf-8")
text = re.sub(r"<!-- checks:start -->.*?<!-- checks:end -->", lambda m: "<!-- checks:start -->\n" + block + "\n<!-- checks:end -->", text, flags=re.S)
text = re.sub(r'(<span data-total>)\d+(</span>)', lambda m: m.group(1) + str(total) + m.group(2), text)
page.write_text(text, encoding="utf-8")
print(f"docs/index.html updated: {total} checks in {len(groups)} categories")
