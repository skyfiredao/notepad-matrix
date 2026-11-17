import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET


REVISION = "d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e"
ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "assets/languages.json"
THEME_OUTPUT = ROOT / "assets/languages.themes.json"
THEME_SOURCES = {
    "light": "PowerEditor/src/stylers.model.xml",
    "dark": "PowerEditor/installer/themes/DarkModeDefault.xml",
}
SOURCE_FILES = [
    "PowerEditor/src/langs.model.xml",
    "PowerEditor/src/stylers.model.xml",
    "PowerEditor/src/ScintillaComponent/ScintillaEditView.cpp",
    "PowerEditor/src/ScintillaComponent/ScintillaEditView.h",
    "PowerEditor/src/ScintillaComponent/Buffer.cpp",
    "PowerEditor/src/Parameters.cpp",
    "PowerEditor/src/Parameters.h",
    "PowerEditor/src/MISC/Common/NppConstants.h",
    "PowerEditor/src/MISC/PluginsManager/Notepad_plus_msgs.h",
    "LICENSE",
]


def frozen(repo, path):
    return subprocess.check_output(["git", "-C", str(repo), "show", f"{REVISION}:{path}"])


def functions(text):
    result = {}
    pattern = r"(?:void\s+(?:ScintillaEditView::)?)(set\w+Lexer|setLexer)\([^)]*\)\s*(?:const\s*)?\{"
    for match in re.finditer(pattern, text):
        start = match.end()
        depth = 1
        end = start
        while depth and end < len(text):
            depth += (text[end] == "{") - (text[end] == "}")
            end += 1
        result[match[1]] = text[start:end - 1]
    return result


def literal_properties(body):
    return dict(re.findall(r'SCI_SETPROPERTY, reinterpret_cast<WPARAM>\("([^"]+)"\), reinterpret_cast<LPARAM>\("([^"]*)"\)', body))


def build(repo):
    raw = {p: frozen(repo, p) for p in SOURCE_FILES}
    text = {p: data.decode("utf-8-sig") for p, data in raw.items()}
    cpp = text[SOURCE_FILES[2]]
    header = text[SOURCE_FILES[3]]
    names = re.findall(r'\{L"([^"]+)",\s*L"([^"]+)",\s*L"([^"]+)",\s*(L_\w+),\s*"([^"]+)"\}', cpp)
    identities = {entry[0]: (i, entry) for i, entry in enumerate(names)}
    enum_text = text[SOURCE_FILES[8]]
    enum_body = re.search(r"enum LangType\s*\{([^}]+)\}", enum_text, re.S)
    if enum_body is None:
        raise ValueError("LangType enum missing")
    enum_symbols = re.findall(r"\bL_\w+\b", re.sub(r"//[^\n]*", "", enum_body[1]))
    if enum_symbols != [entry[3] for entry in names]:
        raise ValueError("language name table does not match LangType discriminants")
    models = ET.fromstring(raw[SOURCE_FILES[0]]).findall(".//Language")
    stylers = {e.get("name"): e for e in ET.fromstring(raw[SOURCE_FILES[1]]).findall(".//LexerType")}
    if len(models) != 96 or len(stylers) != 93 or len(names) != 98:
        raise ValueError(f"frozen source count changed: {len(models)}, {len(stylers)}, {len(names)}")
    funcs = functions(header + "\n" + cpp)
    classes = ["instre1", "instre2", "type1", "type2", "type3", "type4", "type5", "type6", "type7"]
    constants = text[SOURCE_FILES[7]]
    for symbol, index in [("INSTR", 0), ("INSTR2", 1), ("TYPE", 2), ("TYPE2", 3), ("TYPE7", 8), ("SUBSTYLE1", 9)]:
        if not re.search(rf"LANG_INDEX_{symbol}\s*=\s*{index}\b", constants):
            raise ValueError(f"keyword class constant changed: {symbol}")
    model_by_name = {model.get("name"): model for model in models}

    def keyword(source_name, cls, style_name=None, append=True):
        model = model_by_name[source_name]
        raw_words = next((e.text or "" for e in model if e.get("name") == cls), "")
        if not append:
            return raw_words
        style = stylers.get(style_name or source_name)
        style_words = "" if style is None else next((e.text or "" for e in style if e.get("keywordClass") == cls and e.text), "")
        return style_words + " " + raw_words
    dispatch = cpp[cpp.index("switch (typeDoc)"):]
    dispatch = dispatch[:dispatch.index("//All the global styles")]
    setters = {}
    pending = []
    for line in dispatch.splitlines():
        pending.extend(re.findall(r"case\s+(L_\w+)\s*:", line))
        call = re.search(r"(set\w+Lexer)\(([^)]*)\).*break", line)
        if call:
            for symbol in pending:
                setters[symbol] = call[1]
            pending = []
        elif "break;" in line:
            pending = []
    filenames = {}
    buffer = text[SOURCE_FILES[4]]
    refinement = buffer[buffer.index("if (determinedLang == L_TEXT)"):buffer.index("updateTimeStamp();", buffer.index("if (determinedLang == L_TEXT)"))]
    for line, next_line in zip(refinement.splitlines(), refinement.splitlines()[1:]):
        matched_names = re.findall(r'_wcsicmp\(_fileName, L"([^"]+)"\)', line)
        target = re.search(r"determinedLang = (L_\w+)", next_line)
        if matched_names and target:
            filenames.setdefault(target[1], []).extend(matched_names)
    profiles = []
    for order, model in enumerate(models):
        name = model.get("name")
        source_id, (_, label, description, symbol, lexer) = identities[name]
        setter = setters.get(symbol)
        if symbol == "L_SEARCHRESULT":
            setter = "setSearchResultLexer"
        body = funcs.get(setter, "")
        props = {}
        if "setLexer(" in body:
            props.update(literal_properties(funcs["setLexer"]))
        props.update(literal_properties(body))
        if setter == "setCppLexer" and symbol != "L_GOLANG":
            props.pop("lexer.cpp.backquoted.strings", None)
        if setter == "setJsonLexer" and symbol != "L_JSON5":
            props.pop("lexer.json.allow.comments", None)
        if setter == "setSqlLexer":
            if "_backSlashIsEscapeCharacterForSql = true" not in text[SOURCE_FILES[6]]:
                raise ValueError("SQL default changed")
            props["sql.backslash.escapes"] = "1"
        if setter == "setXmlLexer" and symbol != "L_XML":
            props.pop("lexer.xml.allow.scripts", None)
            props.update(literal_properties(funcs["setEmbeddedAspLexer"]))
        style_source = {"json5": "json", "jsp": "html"}.get(name, name)
        style_element = stylers.get(style_source)
        styles = [] if style_element is None else [
            {"id": int(e.get("styleID")), "attributes": dict(e.attrib), "keywords": e.text or ""}
            for e in style_element
        ]
        if name == "normal":
            style_source = None
        components = ["html", "javascript", "php", "asp"] if setter == "setXmlLexer" and symbol != "L_XML" else []
        numbered = [""] * 9
        for index in [int(i) for i in re.findall(r"\bLIST_(\d+)\b", body)]:
            numbered[index] = keyword(name, classes[index])
        keyword_sources = [name]
        if setter in ["setCppLexer", "setJsLexer", "setObjCLexer", "setTypeScriptLexer"]:
            source_name = "javascript.js" if setter == "setJsLexer" else name
            numbered[0] = keyword(source_name, "instre1")
            numbered[1] = keyword(source_name, "type1")
            if setter != "setTypeScriptLexer":
                numbered[3] = keyword(source_name, "instre2")
            keyword_sources = [source_name]
            if symbol != "L_RC":
                numbered[2] = keyword("cpp", "type2", append=False)
                if source_name != "cpp":
                    keyword_sources.append("cpp")
            if setter == "setObjCLexer":
                numbered[4] = keyword(source_name, "type2")
        elif setter == "setJsonLexer":
            numbered[0] = keyword("json", "instre1")
            numbered[1] = keyword("json", "instre2")
            keyword_sources = ["json"]
        elif setter == "setTclLexer":
            numbered = [keyword(name, cls) for cls in classes]
        elif setter == "setBashLexer":
            numbered[0] = keyword(name, "instre1")
        elif setter == "setXmlLexer":
            if symbol == "L_XML":
                numbered[5] = keyword(name, "instre1")
            else:
                numbered[0] = keyword("html", "instre1")
                numbered[1] = keyword("javascript", "instre1")
                numbered[2] = keyword("vb", "instre1", style_name="asp")
                numbered[4] = keyword("php", "instre1")
                numbered[5] = keyword("html", "instre2")
                keyword_sources = components + ["vb"]
        effective_lexer = "hypertext" if components else lexer
        effective_styles = ["javascript.js"] if setter == "setJsLexer" else components or ([style_source] if style_source else [])
        profiles.append({
            "id": source_id,
            "name": name,
            "label": label,
            "description": description,
            "source_symbol": symbol,
            "lexer": lexer,
            "effective_lexer": effective_lexer,
            "order": order,
            "internal": name == "searchResult",
            "attributes": dict(model.attrib),
            "extensions": model.get("ext", "").split(),
            "filenames": filenames.get(symbol, []),
            "keywords": {e.get("name"): e.text or "" for e in model.findall("Keywords")},
            "keyword_sources": keyword_sources,
            "numbered_keywords": numbered,
            "style_source": style_source,
            "style_attributes": dict(style_element.attrib) if style_element is not None else {},
            "styles": styles,
            "effective_style_sources": effective_styles,
            "components": components,
            "properties": props,
            "property_setter": setter,
            "comment_line": model.get("commentLine", ""),
            "comment_start": model.get("commentStart", ""),
            "comment_end": model.get("commentEnd", ""),
        })
    return {
        "schema_version": 1,
        "source_revision": REVISION,
        "source_hashes": {p: hashlib.sha256(data).hexdigest() for p, data in raw.items()},
        "source_model_count": len(models),
        "source_styler_count": len(stylers),
        "profiles": profiles,
    }


def theme_styles(elements):
    return [{"id": int(e.get("styleID")), "attributes": dict(e.attrib), "keywords": e.text or ""} for e in elements]


def build_themes(repo):
    themes = {}
    hashes = {}
    for name, path in THEME_SOURCES.items():
        raw = frozen(repo, path)
        hashes[path] = hashlib.sha256(raw).hexdigest()
        root = ET.fromstring(raw)
        lexers = root.findall(".//LexerType")
        globals = root.findall(".//GlobalStyles/WidgetStyle")
        if len(lexers) != 93 or sum(len(e) for e in lexers) != 1774 or len(globals) != 53:
            raise ValueError(f"frozen theme counts changed: {name}")
        themes[name] = {
            "source_path": path,
            "lexers": [{"attributes": dict(e.attrib), "styles": theme_styles(e)} for e in lexers],
            "global_styles": theme_styles(globals),
        }
    hashes["LICENSE"] = hashlib.sha256(frozen(repo, "LICENSE")).hexdigest()
    return {"schema_version": 1, "source_revision": REVISION, "source_hashes": hashes, "themes": themes}


def audit_themes(repo, actual):
    for name, path in THEME_SOURCES.items():
        root = ET.fromstring(frozen(repo, path))
        source_lexers = root.findall(".//LexerType")
        theme = actual["themes"][name]
        if len(theme["lexers"]) != len(source_lexers):
            raise ValueError(f"theme lexer count differs: {name}")
        for expected, entry in zip(source_lexers, theme["lexers"]):
            if entry["attributes"] != expected.attrib or len(entry["styles"]) != len(expected):
                raise ValueError(f"theme lexer attributes differ: {name}/{expected.get('name')}")
            for source_style, style in zip(expected, entry["styles"]):
                if style["id"] != int(source_style.get("styleID")) or style["attributes"] != source_style.attrib or style["keywords"] != (source_style.text or ""):
                    raise ValueError(f"theme style differs: {name}/{expected.get('name')}/{source_style.get('styleID')}")
        source_globals = root.findall(".//GlobalStyles/WidgetStyle")
        if len(theme["global_styles"]) != len(source_globals):
            raise ValueError(f"theme global count differs: {name}")
        for source_style, style in zip(source_globals, theme["global_styles"]):
            if style["id"] != int(source_style.get("styleID")) or style["attributes"] != source_style.attrib or style["keywords"] != (source_style.text or ""):
                raise ValueError(f"theme global style differs: {name}/{source_style.get('name')}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = build(args.source)
    expected_themes = build_themes(args.source)
    heading = "GNU GENERAL PUBLIC LICENSE"
    notice, body = frozen(args.source, "LICENSE").decode().split(heading, 1)
    if " ".join((heading + body).split()) != " ".join((ROOT / "LICENSE").read_text().split()):
        raise SystemExit("shared GPL text differs from frozen source")
    if " ".join(notice.split()) not in " ".join((ROOT / "THIRD_PARTY_NOTICES.md").read_text().split()):
        raise SystemExit("shared third-party notices omit the frozen upstream copyright and license notice")
    if args.check:
        actual = json.loads(OUTPUT.read_text())
        if actual != expected:
            raise SystemExit("catalog does not equal frozen source models, styles, naming, properties, or filename associations")
        models = ET.fromstring(frozen(args.source, SOURCE_FILES[0])).findall(".//Language")
        stylers = ET.fromstring(frozen(args.source, SOURCE_FILES[1])).findall(".//LexerType")
        by_name = {p["name"]: p for p in actual["profiles"]}
        for model in models:
            profile = by_name[model.get("name")]
            assert profile["attributes"] == model.attrib
            assert profile["keywords"] == {e.get("name"): e.text or "" for e in model}
        for styler in stylers:
            profile = by_name[styler.get("name")]
            assert profile["style_attributes"] == styler.attrib
            assert profile["styles"] == [{"id": int(e.get("styleID")), "attributes": e.attrib, "keywords": e.text or ""} for e in styler]
        keyword_count = sum(len(p["keywords"]) for p in actual["profiles"])
        style_count = sum(len(styler) for styler in stylers)
        print(f"96/96 source models, 93/93 source stylers, {keyword_count} keyword classes, {style_count} style records")
        print("all keyword text/style attributes/properties/identity/associations equal frozen source; LangType discriminants and license verified")
        actual_themes = json.loads(THEME_OUTPUT.read_text())
        if actual_themes != expected_themes:
            raise SystemExit("theme catalog differs from frozen light/dark sources")
        audit_themes(args.source, actual_themes)
        print("light and dark: each 93/93 LexerTypes, 1774/1774 lexer styles, 53/53 global styles; every color/font/style attribute and keyword text equals frozen XML")
    else:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n")
        THEME_OUTPUT.write_text(json.dumps(expected_themes, ensure_ascii=False, indent=2) + "\n")
        print(f"wrote {len(expected['profiles'])} profiles to {OUTPUT}")


if __name__ == "__main__":
    main()
