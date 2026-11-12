import argparse
import json
import re

from language_catalog import ROOT, frozen


SOURCE = "PowerEditor/src/WinControls/AnsiCharPanel/asciiListView.cpp"
OUTPUT = ROOT / "src/character_codes.rs"


def build(repository):
    text = frozen(repository, SOURCE).decode("utf-8-sig")
    controls = dict((int(key), value) for key, value in re.findall(r'case\s+(\d+):\s*return L"([^\"]*)";', text[text.index("AsciiListView::getAscii"):text.index("AsciiListView::getHtmlName")]))
    names = dict((int(key), value) for key, value in re.findall(r'case\s+(\d+):\s*return L"([^\"]*)";', text[text.index("AsciiListView::getHtmlName"):text.index("AsciiListView::getHtmlNumber")]))
    numbers = dict((int(key), int(value)) for key, value in re.findall(r'case\s+(\d+):\s*return (\d+);', text[text.index("AsciiListView::getHtmlNumber"):text.index("AsciiListView::setValues")]))
    arrays = []
    for name, kind, values in [
        ("CONTROL_NAMES", "&str", [json.dumps(controls.get(i, "")) for i in range(256)]),
        ("HTML_NAMES", "&str", [json.dumps(names.get(i, "")) for i in range(256)]),
        ("HTML_NUMBERS", "i32", [str(numbers.get(i, -1)) for i in range(256)]),
    ]:
        lines = ["    " + ", ".join(values[i:i + 8]) + "," for i in range(0, 256, 8)]
        arrays.append(f"pub const {name}: [{kind}; 256] = [\n" + "\n".join(lines) + "\n];\n")
    return "\n".join(arrays)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("repository")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = build(args.repository)
    if args.check:
        tokens = lambda text: re.findall(r'"(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z_0-9]*|-?\d+|[^\s]', text)
        if tokens(OUTPUT.read_text()) != tokens(output):
            raise SystemExit("Frozen character table differs")
    else:
        OUTPUT.write_text(output)
