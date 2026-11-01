"""Render the downloaded manufacturer schematic for pin/circuit inspection."""
from pathlib import Path
import pymupdf

root = Path(__file__).resolve().parents[1]
doc = pymupdf.open(root / "tmp/pdfs/schematic.pdf")
page = doc[0]
page.get_pixmap(matrix=pymupdf.Matrix(2, 2)).save(root / "tmp/pdfs/schematic.png")
for term in ("BL", "LCD", "IO6"):
    for i, rect in enumerate(page.search_for(term)):
        clip = pymupdf.Rect(rect.x0 - 120, rect.y0 - 100, rect.x1 + 120, rect.y1 + 100) & page.rect
        page.get_pixmap(matrix=pymupdf.Matrix(3, 3), clip=clip).save(root / f"tmp/pdfs/{term}-{i}.png")
