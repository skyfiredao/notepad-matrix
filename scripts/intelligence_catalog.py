import argparse
import hashlib
import json
import pathlib
import re
import xml.etree.ElementTree as ET


def xml_root(path, preserve_lines=False):
    text = path.read_text(encoding='utf-8-sig')
    if preserve_lines:
        text = re.sub(r'<!--.*?-->', '', text, flags=re.S)
        text = re.sub(r'="[^"]*"', lambda m: m[0].replace('\n', '&#10;').replace('\t', '&#9;').replace('\r', '&#13;'), text)
    return ET.fromstring(text)


def node(element):
    return {'tag': element.tag, 'attributes': dict(element.attrib),
            'text': element.text or '', 'children': [node(child) for child in element]}


def generate(source):
    apis = {}
    parsers = {}
    for path in sorted((source / 'APIs').glob('*.xml')):
        apis[path.stem.lower()] = node(xml_root(path).find('AutoComplete'))
    for path in sorted((source / 'functionList').glob('*.xml')):
        parser = xml_root(path, True).find('.//parser')
        if parser is not None:
            parsers[path.stem] = {'source_file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                                  'definition': node(parser)}
    overrides = node(xml_root(source / 'functionList/overrideMap.xml').find('.//associationMap'))
    return {'apis': apis, 'parsers': parsers, 'overrides': overrides}


def load_committed(directory):
    return {key: json.loads((directory / (key + '.json')).read_text()) for key in ('apis', 'parsers', 'overrides')}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1] / 'assets/intelligence')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    catalog = generate(args.source)
    if args.check:
        if catalog != load_committed(args.output):
            raise SystemExit('intelligence catalog differs from source fields')
        print('all API keyword/overload/parameter/environment fields and all function-list definitions match source')
    else:
        args.output.mkdir(parents=True, exist_ok=True)
        for key, value in catalog.items():
            (args.output / (key + '.json')).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
        print(f"generated {len(catalog['apis'])} API catalogs and {len(catalog['parsers'])} parser definitions")


if __name__ == '__main__':
    main()
