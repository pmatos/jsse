#!/usr/bin/env python3
"""Generate src/unicode_tables.rs from Unicode 17.0.0 UCD data files."""

import os
import re
import sys
import urllib.request
from collections import defaultdict
from pathlib import Path

UCD_BASE = "https://www.unicode.org/Public/17.0.0/ucd/"

UCD_FILES = {
    "Scripts.txt": "Scripts.txt",
    "ScriptExtensions.txt": "ScriptExtensions.txt",
    "DerivedGeneralCategory.txt": "extracted/DerivedGeneralCategory.txt",
    "DerivedCoreProperties.txt": "DerivedCoreProperties.txt",
    "PropList.txt": "PropList.txt",
    "emoji-data.txt": "emoji/emoji-data.txt",
    "PropertyValueAliases.txt": "PropertyValueAliases.txt",
}

CACHE_DIR = Path(__file__).parent / ".unicode_cache"


def download_file(name: str, url_path: str) -> str:
    CACHE_DIR.mkdir(exist_ok=True)
    cached = CACHE_DIR / name
    if cached.exists():
        return cached.read_text(encoding="utf-8")
    url = UCD_BASE + url_path
    print(f"Downloading {url} ...", file=sys.stderr)
    with urllib.request.urlopen(url) as resp:
        data = resp.read().decode("utf-8")
    cached.write_text(data, encoding="utf-8")
    return data


def parse_ranges(text: str) -> dict[str, list[tuple[int, int]]]:
    """Parse UCD file into {property_value: [(start, end), ...]}."""
    result = defaultdict(list)
    for line in text.splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        parts = line.split(";")
        if len(parts) < 2:
            continue
        range_str = parts[0].strip()
        prop_val = parts[1].strip()
        if ".." in range_str:
            lo, hi = range_str.split("..")
            lo, hi = int(lo, 16), int(hi, 16)
        else:
            lo = hi = int(range_str, 16)
        result[prop_val].append((lo, hi))
    # Sort and merge ranges
    for key in result:
        result[key] = merge_ranges(sorted(result[key]))
    return result


def merge_ranges(ranges: list[tuple[int, int]]) -> list[tuple[int, int]]:
    if not ranges:
        return ranges
    merged = [ranges[0]]
    for lo, hi in ranges[1:]:
        prev_lo, prev_hi = merged[-1]
        if lo <= prev_hi + 1:
            merged[-1] = (prev_lo, max(prev_hi, hi))
        else:
            merged.append((lo, hi))
    return merged


def parse_script_extensions(text: str, script_data: dict[str, list[tuple[int, int]]]) -> dict[str, list[tuple[int, int]]]:
    """Parse ScriptExtensions.txt. Each codepoint gets added to all listed scripts.
    Also inherits from Script property for codepoints not in ScriptExtensions."""
    scx = defaultdict(list)
    # Track which codepoints are covered by ScriptExtensions.txt
    scx_covered = set()

    for line in text.splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        parts = line.split(";")
        if len(parts) < 2:
            continue
        range_str = parts[0].strip()
        scripts_str = parts[1].strip()
        if ".." in range_str:
            lo, hi = range_str.split("..")
            lo, hi = int(lo, 16), int(hi, 16)
        else:
            lo = hi = int(range_str, 16)
        scripts = scripts_str.split()
        for sc in scripts:
            scx[sc].append((lo, hi))
        for cp in range(lo, hi + 1):
            scx_covered.add(cp)

    # For codepoints NOT in ScriptExtensions, inherit from Script property
    for script_name, ranges in script_data.items():
        if script_name in ("Common", "Inherited"):
            continue
        for lo, hi in ranges:
            for cp in range(lo, hi + 1):
                if cp not in scx_covered:
                    scx[script_name].append((cp, cp))

    # Also, Common and Inherited codepoints that are NOT in ScriptExtensions
    # get their Script value as their Script_Extensions value
    for script_name in ("Common", "Inherited"):
        if script_name in script_data:
            for lo, hi in script_data[script_name]:
                for cp in range(lo, hi + 1):
                    if cp not in scx_covered:
                        scx[script_name].append((cp, cp))

    for key in scx:
        scx[key] = merge_ranges(sorted(scx[key]))
    return dict(scx)


def parse_property_value_aliases(text: str) -> dict[str, dict[str, list[str]]]:
    """Parse PropertyValueAliases.txt -> {property: {canonical: [aliases]}}."""
    result = defaultdict(lambda: defaultdict(list))
    for line in text.splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        parts = [p.strip() for p in line.split(";")]
        if len(parts) < 3:
            continue
        prop = parts[0]
        short_name = parts[1]
        long_name = parts[2]
        # Use long_name as canonical
        all_names = [short_name, long_name] + parts[3:]
        # Remove empty entries
        all_names = [n for n in all_names if n]
        result[prop][long_name] = all_names
    return dict(result)


def compute_complement(ranges: list[tuple[int, int]]) -> list[tuple[int, int]]:
    """Complement of ranges within [0, 0x10FFFF], excluding surrogates."""
    # First compute full complement
    comp = []
    prev = 0
    for lo, hi in ranges:
        if prev < lo:
            comp.append((prev, lo - 1))
        prev = hi + 1
    if prev <= 0x10FFFF:
        comp.append((prev, 0x10FFFF))
    # Now remove surrogate range
    result = []
    for lo, hi in comp:
        if hi < 0xD800 or lo > 0xDFFF:
            result.append((lo, hi))
        elif lo < 0xD800 and hi > 0xDFFF:
            result.append((lo, 0xD7FF))
            result.append((0xE000, hi))
        elif lo < 0xD800:
            result.append((lo, 0xD7FF))
        elif hi > 0xDFFF:
            result.append((0xE000, hi))
        # else: entirely within surrogates, skip
    return result


def remove_surrogates(ranges: list[tuple[int, int]]) -> list[tuple[int, int]]:
    """Remove surrogate range from ranges (for char class expansion)."""
    result = []
    for lo, hi in ranges:
        if hi < 0xD800 or lo > 0xDFFF:
            result.append((lo, hi))
        elif lo < 0xD800 and hi > 0xDFFF:
            result.append((lo, 0xD7FF))
            result.append((0xE000, hi))
        elif lo < 0xD800:
            result.append((lo, 0xD7FF))
        elif hi > 0xDFFF:
            result.append((0xE000, hi))
    return result


def format_ranges(ranges: list[tuple[int, int]]) -> str:
    parts = []
    for lo, hi in ranges:
        parts.append(f"(0x{lo:04X}, 0x{hi:04X})")
    return ", ".join(parts)


def to_rust_ident(name: str) -> str:
    """Convert a property value name to a valid Rust identifier."""
    return name.replace("-", "_").replace(" ", "_")


def generate_rust(
    script_data: dict,
    scx_data: dict,
    gc_data: dict,
    binary_data: dict,
    aliases: dict,
    script_aliases: dict,
    gc_aliases: dict,
) -> str:
    lines = []
    lines.append("// AUTO-GENERATED by scripts/generate_unicode_tables.py")
    lines.append("// Unicode 17.0.0 — do not edit manually.")
    lines.append("")
    lines.append("#![allow(clippy::unreadable_literal)]")
    lines.append("")

    all_tables = {}  # Maps (category, canonical_name) -> rust_const_name

    # Script tables
    for name, ranges in sorted(script_data.items()):
        const_name = f"SCRIPT_{to_rust_ident(name).upper()}"
        ranges_no_surr = remove_surrogates(ranges)
        lines.append(f"const {const_name}: &[(u32, u32)] = &[{format_ranges(ranges_no_surr)}];")
        all_tables[("Script", name)] = const_name

    lines.append("")

    # Script_Extensions tables
    for name, ranges in sorted(scx_data.items()):
        const_name = f"SCX_{to_rust_ident(name).upper()}"
        ranges_no_surr = remove_surrogates(ranges)
        lines.append(f"const {const_name}: &[(u32, u32)] = &[{format_ranges(ranges_no_surr)}];")
        all_tables[("Script_Extensions", name)] = const_name

    lines.append("")

    # General_Category tables
    # Also compute composite categories
    gc_composites = {
        "LC": ["Lu", "Ll", "Lt"],
        "L": ["Lu", "Ll", "Lt", "Lm", "Lo"],
        "M": ["Mn", "Mc", "Me"],
        "N": ["Nd", "Nl", "No"],
        "P": ["Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po"],
        "S": ["Sm", "Sc", "Sk", "So"],
        "Z": ["Zs", "Zl", "Zp"],
        "C": ["Cc", "Cf", "Cs", "Co", "Cn"],
    }
    for name, ranges in sorted(gc_data.items()):
        const_name = f"GC_{to_rust_ident(name).upper()}"
        ranges_no_surr = remove_surrogates(ranges)
        lines.append(f"const {const_name}: &[(u32, u32)] = &[{format_ranges(ranges_no_surr)}];")
        all_tables[("General_Category", name)] = const_name

    # Compute composites from the parsed data
    for comp_name, members in gc_composites.items():
        if comp_name not in gc_data:
            combined = []
            for m in members:
                if m in gc_data:
                    combined.extend(gc_data[m])
            combined = merge_ranges(sorted(combined))
            const_name = f"GC_{to_rust_ident(comp_name).upper()}"
            ranges_no_surr = remove_surrogates(combined)
            lines.append(f"const {const_name}: &[(u32, u32)] = &[{format_ranges(ranges_no_surr)}];")
            all_tables[("General_Category", comp_name)] = const_name

    lines.append("")

    # Binary property tables
    for name, ranges in sorted(binary_data.items()):
        const_name = f"BINARY_{to_rust_ident(name).upper()}"
        ranges_no_surr = remove_surrogates(ranges)
        lines.append(f"const {const_name}: &[(u32, u32)] = &[{format_ranges(ranges_no_surr)}];")
        all_tables[("Binary", name)] = const_name

    # Special binary properties
    # ASCII
    if ("Binary", "ASCII") not in all_tables:
        lines.append("const BINARY_ASCII: &[(u32, u32)] = &[(0x0000, 0x007F)];")
        all_tables[("Binary", "ASCII")] = "BINARY_ASCII"

    # Any
    lines.append("const BINARY_ANY: &[(u32, u32)] = &[(0x0000, 0xD7FF), (0xE000, 0x10FFFF)];")
    all_tables[("Binary", "Any")] = "BINARY_ANY"

    # Assigned = complement of Unassigned (gc=Cn)
    if "Cn" in gc_data:
        assigned_ranges = compute_complement(gc_data["Cn"])
        lines.append(f"const BINARY_ASSIGNED: &[(u32, u32)] = &[{format_ranges(assigned_ranges)}];")
        all_tables[("Binary", "Assigned")] = "BINARY_ASSIGNED"

    lines.append("")

    # Build the lookup function
    lines.append("pub fn lookup_property(content: &str) -> Option<&'static [(u32, u32)]> {")
    lines.append("    if let Some(eq_pos) = content.find('=') {")
    lines.append("        let prop_name = &content[..eq_pos];")
    lines.append("        let prop_value = &content[eq_pos + 1..];")
    lines.append("        match prop_name {")
    lines.append('            "Script" | "sc" => lookup_script(prop_value),')
    lines.append('            "Script_Extensions" | "scx" => lookup_script_extensions(prop_value),')
    lines.append('            "General_Category" | "gc" => lookup_gc(prop_value),')
    lines.append("            _ => None,")
    lines.append("        }")
    lines.append("    } else {")
    lines.append("        // Try binary property first, then lone GC value")
    lines.append("        if let Some(r) = lookup_binary(content) {")
    lines.append("            return Some(r);")
    lines.append("        }")
    lines.append("        lookup_gc(content)")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    # lookup_script
    lines.append("fn lookup_script(value: &str) -> Option<&'static [(u32, u32)]> {")
    lines.append("    match value {")
    for name in sorted(script_data.keys()):
        const_name = all_tables[("Script", name)]
        # Add aliases
        names = {name}
        for canonical, alias_list in script_aliases.items():
            if canonical == name:
                names.update(alias_list)
            elif name in alias_list:
                names.add(canonical)
                names.update(alias_list)
        for n in sorted(names):
            lines.append(f'        "{n}" => Some({const_name}),')
    lines.append("        _ => None,")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    # lookup_script_extensions
    lines.append("fn lookup_script_extensions(value: &str) -> Option<&'static [(u32, u32)]> {")
    lines.append("    match value {")
    for name in sorted(scx_data.keys()):
        const_name = all_tables[("Script_Extensions", name)]
        # Use same aliases as Script
        names = {name}
        for canonical, alias_list in script_aliases.items():
            if canonical == name:
                names.update(alias_list)
            elif name in alias_list:
                names.add(canonical)
                names.update(alias_list)
        for n in sorted(names):
            lines.append(f'        "{n}" => Some({const_name}),')
    lines.append("        _ => None,")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    # lookup_gc
    lines.append("fn lookup_gc(value: &str) -> Option<&'static [(u32, u32)]> {")
    lines.append("    match value {")
    # Build GC alias map
    gc_alias_map = {}
    for canonical, alias_list in gc_aliases.items():
        for a in alias_list:
            gc_alias_map[a] = canonical
        gc_alias_map[canonical] = canonical

    emitted_gc = set()
    for cat, name in sorted(all_tables.keys()):
        if cat != "General_Category":
            continue
        const_name = all_tables[("General_Category", name)]
        names = {name}
        # Add long name aliases
        for canonical, alias_list in gc_aliases.items():
            if canonical == name or name in alias_list:
                names.add(canonical)
                names.update(alias_list)
        for n in sorted(names):
            if n not in emitted_gc:
                lines.append(f'        "{n}" => Some({const_name}),')
                emitted_gc.add(n)
    # Also add "punct" alias for Punctuation
    if "punct" not in emitted_gc and ("General_Category", "P") in all_tables:
        lines.append(f'        "punct" => Some({all_tables[("General_Category", "P")]}),')
    lines.append("        _ => None,")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    # lookup_binary
    lines.append("fn lookup_binary(name: &str) -> Option<&'static [(u32, u32)]> {")
    lines.append("    match name {")

    # Build binary alias map from PropertyValueAliases
    binary_alias_map = defaultdict(set)
    # The binary properties in PropertyValueAliases are under property "sc", "gc", etc.
    # but for actual binary properties, they're self-named. We need to add common aliases.
    binary_aliases = {
        "ASCII_Hex_Digit": ["AHex"],
        "Alphabetic": ["Alpha"],
        "Bidi_Control": ["Bidi_C"],
        "Bidi_Mirrored": ["Bidi_M"],
        "Case_Ignorable": ["CI"],
        "Cased": [],
        "Changes_When_Casefolded": ["CWCF"],
        "Changes_When_Casemapped": ["CWCM"],
        "Changes_When_Lowercased": ["CWL"],
        "Changes_When_NFKC_Casefolded": ["CWKCF"],
        "Changes_When_Titlecased": ["CWT"],
        "Changes_When_Uppercased": ["CWU"],
        "Dash": [],
        "Default_Ignorable_Code_Point": ["DI"],
        "Deprecated": ["Dep"],
        "Diacritic": ["Dia"],
        "Emoji": [],
        "Emoji_Component": ["EComp"],
        "Emoji_Modifier": ["EMod"],
        "Emoji_Modifier_Base": ["EBase"],
        "Emoji_Presentation": ["EPres"],
        "Extended_Pictographic": ["ExtPict"],
        "Extender": ["Ext"],
        "Grapheme_Base": ["Gr_Base"],
        "Grapheme_Extend": ["Gr_Ext"],
        "Hex_Digit": ["Hex"],
        "IDS_Binary_Operator": ["IDSB"],
        "IDS_Trinary_Operator": ["IDST"],
        "IDS_Unary_Operator": ["IDSU"],
        "ID_Continue": ["IDC"],
        "ID_Start": ["IDS"],
        "Ideographic": ["Ideo"],
        "Join_Control": ["Join_C"],
        "Logical_Order_Exception": ["LOE"],
        "Lowercase": ["Lower"],
        "Math": [],
        "Noncharacter_Code_Point": ["NChar"],
        "Pattern_Syntax": ["Pat_Syn"],
        "Pattern_White_Space": ["Pat_WS"],
        "Quotation_Mark": ["QMark"],
        "Radical": [],
        "Regional_Indicator": ["RI"],
        "Sentence_Terminal": ["STerm"],
        "Soft_Dotted": ["SD"],
        "Terminal_Punctuation": ["Term"],
        "Unified_Ideograph": ["UIdeo"],
        "Uppercase": ["Upper"],
        "Variation_Selector": ["VS"],
        "White_Space": ["space", "WSpace"],
        "XID_Continue": ["XIDC"],
        "XID_Start": ["XIDS"],
        "ASCII": [],
        "Any": [],
        "Assigned": [],
    }

    emitted_binary = set()
    for name in sorted(all_tables.keys()):
        cat, prop_name = name
        if cat != "Binary":
            continue
        const_name = all_tables[name]
        names = {prop_name}
        if prop_name in binary_aliases:
            names.update(binary_aliases[prop_name])
        for n in sorted(names):
            if n not in emitted_binary:
                lines.append(f'        "{n}" => Some({const_name}),')
                emitted_binary.add(n)

    lines.append("        _ => None,")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    return "\n".join(lines)


def main():
    # Download UCD files
    texts = {}
    for name, url_path in UCD_FILES.items():
        texts[name] = download_file(name, url_path)

    # Parse Script property
    script_data = parse_ranges(texts["Scripts.txt"])
    print(f"Scripts: {len(script_data)} values", file=sys.stderr)

    # Parse Script_Extensions
    # ScriptExtensions.txt uses short script names, we need to map them
    pva = parse_property_value_aliases(texts["PropertyValueAliases.txt"])

    # Build short->long script name map
    script_short_to_long = {}
    script_long_to_short = {}
    if "sc" in pva:
        for long_name, alias_list in pva["sc"].items():
            for a in alias_list:
                script_short_to_long[a] = long_name
            script_short_to_long[long_name] = long_name
            if len(alias_list) >= 1:
                script_long_to_short[long_name] = alias_list[0]

    # Parse raw ScriptExtensions data (uses short names)
    scx_raw = parse_script_extensions(texts["ScriptExtensions.txt"], script_data)
    # Convert short script names to long names where possible
    scx_data = {}
    for name, ranges in scx_raw.items():
        long_name = script_short_to_long.get(name, name)
        if long_name in scx_data:
            scx_data[long_name] = merge_ranges(sorted(scx_data[long_name] + ranges))
        else:
            scx_data[long_name] = ranges
    print(f"Script_Extensions: {len(scx_data)} values", file=sys.stderr)

    # Parse General_Category
    gc_data = parse_ranges(texts["DerivedGeneralCategory.txt"])
    print(f"General_Category: {len(gc_data)} values", file=sys.stderr)

    # Parse binary properties from multiple sources
    binary_data = {}

    core_props = parse_ranges(texts["DerivedCoreProperties.txt"])
    for name, ranges in core_props.items():
        binary_data[name] = ranges

    prop_list = parse_ranges(texts["PropList.txt"])
    for name, ranges in prop_list.items():
        binary_data[name] = ranges

    emoji_data = parse_ranges(texts["emoji-data.txt"])
    for name, ranges in emoji_data.items():
        binary_data[name] = ranges

    print(f"Binary properties: {len(binary_data)} values", file=sys.stderr)

    # Build alias maps
    script_aliases = pva.get("sc", {})
    gc_aliases = pva.get("gc", {})

    # Generate Rust source
    rust_src = generate_rust(
        script_data, scx_data, gc_data, binary_data,
        pva, script_aliases, gc_aliases,
    )

    out_path = Path(__file__).parent.parent / "src" / "unicode_tables.rs"
    out_path.write_text(rust_src, encoding="utf-8")
    size_kb = out_path.stat().st_size / 1024
    print(f"Generated {out_path} ({size_kb:.0f} KB)", file=sys.stderr)


if __name__ == "__main__":
    main()
