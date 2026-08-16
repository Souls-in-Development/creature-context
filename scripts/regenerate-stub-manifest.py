#!/usr/bin/env python3
"""Regenerate docs/stub-manifest.txt from the current tree.

The anti-facade gate (crates/context-cli/tests/no_facade.rs) fails on a stale
entry as well as an undeclared one, so any task that implements a declared
facade must run this in the same commit. Run from the repository root.

Detection mirrors the gate exactly. If the two ever disagree, the gate is
authoritative and this script is wrong.
"""

import os
import re
import sys

CONST_RETURNS = re.compile(
    r"^(Ok\(\(\)\)|Ok\(String::new\(\)\)|Ok\(vec!\[\]\)|Ok\(Vec::new\(\)\)|"
    r"String::new\(\)|Vec::new\(\)|vec!\[\]|None|false)$"
)

HEADER = """# Known facades awaiting a later milestone.
#
# The anti-facade gate (crates/context-cli/tests/no_facade.rs) fails if a facade
# appears that is not on this list, and fails if a listed entry is no longer a
# facade. The list may shrink. It may never grow.
#
# Regenerate with: python3 scripts/regenerate-stub-manifest.py
#
# Two entry kinds:
#   path            whole file is a stub (every item has an empty body)
#   path::function  one function in an otherwise real file is trivially constant
#
# Milestone 2 (resident runtime):   crates/context-runtime/
# Milestone 3 (authority/universe): context-core/{authority,context,universe,sources}/,
#                                   permission *, register, context-store/src/universe.rs
# Milestone 4 (structural depth):   crates/context-parsers/
# Milestone 5 (semantic/platform):  crates/context-model/, crates/context-ffi/

"""


def is_stub_file(source: str) -> bool:
    """Every item has an empty body. A mod.rs of `pub mod` lines is not a stub."""
    saw_item = False
    for line in source.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//") or stripped.startswith("#"):
            continue
        if stripped.startswith("pub mod ") or stripped.startswith("mod "):
            continue
        if stripped.endswith("{}") and any(
            kw in stripped for kw in ("fn ", "struct ", "enum ")
        ):
            saw_item = True
            continue
        return False
    return saw_item


def constant_body_functions(source: str):
    """(name, description) for each trivially constant function.

    Trait method declarations have no body and are skipped.
    """
    lines = source.splitlines()
    found = []
    index = 0
    while index < len(lines):
        match = re.search(r"\bfn\s+([A-Za-z0-9_]+)", lines[index])
        if not match:
            index += 1
            continue

        probe, declaration_only = index, False
        while probe < len(lines):
            if "{" in lines[probe]:
                break
            if ";" in lines[probe]:
                declaration_only = True
                break
            probe += 1
        if declaration_only:
            index = probe + 1
            continue

        cursor, depth, started, body = index, 0, False, []
        while cursor < len(lines):
            depth += lines[cursor].count("{")
            if depth > 0:
                started = True
            depth -= lines[cursor].count("}")
            if started:
                body.append(lines[cursor])
                if depth == 0:
                    break
            cursor += 1

        text = "\n".join(body)
        inner = text[text.find("{") + 1 : text.rfind("}")] if "{" in text else ""
        statements = [
            line.strip()
            for line in inner.splitlines()
            if line.strip() and not line.strip().startswith("//")
        ]

        description = None
        if not statements:
            description = "empty body"
        elif len(statements) == 1 and CONST_RETURNS.match(statements[0]):
            description = f"returns {statements[0]}"
        elif (
            len(statements) <= 3
            and "Ok(())" in statements
            and all(
                re.match(r"^(println!|eprintln!|Ok\(\(\)\))", s) for s in statements
            )
        ):
            description = "prints then returns Ok(())"

        if description:
            found.append((match.group(1), description))
        index = max(cursor, index + 1)
    return found


def main() -> int:
    root = os.getcwd()
    if not os.path.isdir(os.path.join(root, "crates")):
        print("run from the repository root", file=sys.stderr)
        return 1

    stub_files, const_funcs = [], []
    for dirpath, dirnames, filenames in os.walk(os.path.join(root, "crates")):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git", ".build")]
        for filename in sorted(filenames):
            if not filename.endswith(".rs"):
                continue
            path = os.path.join(dirpath, filename)
            relative = os.path.relpath(path, root).replace("\\", "/")
            if "/tests/" in relative:
                continue
            with open(path, encoding="utf-8") as handle:
                source = handle.read()
            if is_stub_file(source):
                stub_files.append(relative)
                continue
            for name, description in constant_body_functions(source):
                const_funcs.append((f"{relative}::{name}", description))

    stub_files.sort()
    const_funcs.sort()

    body = "# --- whole-file stubs ---\n" + "\n".join(stub_files)
    body += "\n\n# --- constant-body functions in otherwise real files ---\n"
    body += "\n".join(f"{key}    # {desc}" for key, desc in const_funcs)

    with open("docs/stub-manifest.txt", "w", encoding="utf-8") as handle:
        handle.write(HEADER + body + "\n")

    print(f"{len(stub_files)} whole-file stubs, {len(const_funcs)} constant-body functions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
