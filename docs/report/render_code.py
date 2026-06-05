#!/usr/bin/env python3
"""Render a source code file to PNG using Pygments ImageFormatter."""
import sys
import os
from pathlib import Path

# Pillow installed to ~/pylibs
_pylibs = str(Path.home() / "pylibs")
if _pylibs not in sys.path:
    sys.path.insert(0, _pylibs)

from pygments import highlight
from pygments.formatters import ImageFormatter
from pygments.lexers import get_lexer_by_name, guess_lexer
from pygments.styles import get_style_by_name  # friendly: light grey bg, good contrast for print

def render(src: str, lang: str, out: str) -> None:
    code = Path(src).read_text(encoding="utf-8")
    try:
        lexer = get_lexer_by_name(lang, stripall=True)
    except Exception:
        lexer = guess_lexer(code)

    style = get_style_by_name("xcode")

    formatter = ImageFormatter(
        style=style,
        font_name="Cascadia Code",
        font_size=48,
        line_numbers=True,
        line_number_bg="#f0f0f0",
        line_number_fg="#888888",
        image_pad=48,
        line_pad=12,
    )

    result = highlight(code, lexer, formatter)
    Path(out).write_bytes(result)

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: render_code.py <src> <lang> <out.png>")
        sys.exit(1)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
