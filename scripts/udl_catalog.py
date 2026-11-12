import argparse
import hashlib
import json
import re
import xml.etree.ElementTree as ET

from language_catalog import ROOT, REVISION, frozen


OUTPUT = ROOT / "assets/udls.json"
SOURCES = [
    "PowerEditor/bin/userDefineLangs/markdown._preinstalled.udl.xml",
    "PowerEditor/bin/userDefineLangs/markdown._preinstalled_DM.udl.xml",
]


def tagged(text, count):
    result = [[] for _ in range(count)]
    matches = list(re.finditer(r"(?<!\S)(\d{2})", text))
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        value = text[match.end():end].strip()
        result[int(match[1])].append(value)
    return [" ".join(values) for values in result]


def build(repo):
    definitions = []
    sources = []
    for path in SOURCES:
        raw = frozen(repo, path)
        node = ET.fromstring(raw).find("UserLang")
        settings = node.find("Settings/Global").attrib
        prefix = node.find("Settings/Prefix").attrib
        words = {item.attrib["name"]: item.text or "" for item in node.findall("KeywordLists/Keywords")}
        comments = tagged(words["Comments"], 5)
        delimiters = tagged(words["Delimiters"], 24)
        styles = []
        for index, item in enumerate(node.findall("Styles/WordsStyle")):
            value = item.attrib
            styles.append({
                "id": index, "name": value["name"],
                "foreground": int(value["fgColor"], 16),
                "background": int(value["bgColor"], 16),
                "color_style": int(value.get("colorStyle", "3")),
                "font_enabled": bool(value.get("fontName", "")),
                "font_name": value.get("fontName", ""),
                "font_style": int(value.get("fontStyle", "-1")),
                "font_size": int(value.get("fontSize", "-1")),
                "nesting": int(value.get("nesting", "0")),
            })
        if len(styles) != 24:
            raise ValueError(f"Expected 24 UDL styles: {path}")
        definitions.append({
            "name": node.attrib["name"], "extensions": node.attrib["ext"],
            "udl_version": node.attrib["udlVersion"],
            "dark_mode_theme": node.get("darkModeTheme") == "yes",
            "case_ignored": settings["caseIgnored"] == "yes",
            "allow_fold_comments": settings["allowFoldOfComments"] == "yes",
            "fold_compact": settings["foldCompact"] == "yes",
            "line_comment_placement": int(settings["forcePureLC"]),
            "decimal_separator": int(settings["decimalSeparator"]),
            "prefix_keywords": [prefix[f"Keywords{i}"] == "yes" for i in range(1, 9)],
            "keywords": [words[f"Keywords{i}"] for i in range(1, 9)],
            "operators": [words[f"Operators{i}"] for i in range(1, 3)],
            "comments": dict(zip(["line_open", "line_continue", "line_close", "block_open", "block_close"], comments)),
            "numbers": {key: words[f"Numbers, {key}"] for key in ["prefix1", "prefix2", "extras1", "extras2", "suffix1", "suffix2", "range"]},
            "delimiters": [dict(zip(["open", "escape", "close"], delimiters[i:i + 3])) for i in range(0, 24, 3)],
            "folding": [{key: words[f"Folders in {group}, {key}"] for key in ["open", "middle", "close"]} for group in ["code1", "code2", "comment"]],
            "styles": styles,
        })
        sources.append({"path": path, "sha256": hashlib.sha256(raw).hexdigest()})
    return {"schema_version": 1, "source_revision": REVISION, "sources": sources, "definitions": definitions}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("repository")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = json.dumps(build(args.repository), ensure_ascii=False, indent=2) + "\n"
    if args.check:
        if OUTPUT.read_text() != output:
            raise SystemExit("Frozen UDL catalog differs")
    else:
        OUTPUT.write_text(output)
