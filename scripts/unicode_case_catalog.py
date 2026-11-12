import argparse
import ast
import re
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("source", type=Path)
parser.add_argument("--check", action="store_true")
args = parser.parse_args()
revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=args.source, text=True).strip()
if revision != "d87f8bd8db9baebbfd38b580eb22c7eb05cd2b7e":
    raise SystemExit("expected the frozen Notepad++ 8.9.9 source")
source = subprocess.check_output(["git", "show", f"{revision}:scintilla/src/CaseConvert.cxx"], cwd=args.source, text=True)

def numbers(name):
    section = source.split(f"constexpr int {name}[] = {{", 1)[1].split("};", 1)[0]
    section = re.sub(r"//[^\n]*", "", section)
    return [int(value) for value in re.findall(r"\d+", section)]

folded = {}
ranges = numbers("symmetricCaseConversionRanges")
assert len(ranges) % 4 == 0
for index in range(0, len(ranges), 4):
    lower, upper, length, pitch = ranges[index:index + 4]
    for offset in range(0, length * pitch, pitch):
        folded[upper + offset] = chr(lower + offset)
singletons = numbers("symmetricCaseConversions")
assert len(singletons) % 2 == 0
for lower, upper in zip(singletons[::2], singletons[1::2]):
    folded[upper] = chr(lower)
complex_section = source.split("constexpr std::string_view complexCaseConversions =", 1)[1].split(";", 1)[0]
fields = b"".join(ast.literal_eval("b" + literal) for literal in re.findall(r'"(?:[^"\\]|\\.)*"', complex_section)).decode("utf-8").split("|")
assert fields.pop() == ""
assert len(fields) % 4 == 0
for index in range(0, len(fields), 4):
    origin, fold, _, _ = fields[index:index + 4]
    assert len(origin) == 1
    if fold:
        folded[ord(origin)] = fold

def rust_string(text):
    return '"' + "".join(f"\\u{{{ord(ch):x}}}" for ch in text) + '"'

rows = [f"    (0x{point:x}, {rust_string(text)})," for point, text in sorted(folded.items())]
generated = "const MAPPINGS: &[(u32, &str)] = &[\n" + "\n".join(rows) + "\n];\n\n"
generated += "pub(super) fn mapping(ch: char) -> Option<&'static str> {\n    MAPPINGS\n        .binary_search_by_key(&(ch as u32), |&(point, _)| point)\n        .ok()\n        .map(|index| MAPPINGS[index].1)\n}\n"
target = Path(__file__).resolve().parents[1] / "src/unicode_case.rs"
if args.check:
    if target.read_text() != generated:
        raise SystemExit("Unicode case table differs from the frozen source")
else:
    target.write_text(generated)
print(f"{len(folded)} frozen Unicode folding mappings: {'verified' if args.check else 'generated'}")
