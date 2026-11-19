"""Try markdown_to_pdf on a sample: python spike_md_pdf.py out.pdf [theme]. Writes the PDF and a PNG of each page."""
import sys
from pathlib import Path

import pyegui

TEXT = """# Rapport de synthèse

Un paragraphe avec du **gras**, de l'*italique*, du `code en ligne`, un [lien](https://github.com/emilk/egui)
et des accents : é è ê à ç ù œ — « guillemets » €.

## Liste

- premier élément
- deuxième élément
  - un sous-élément
- troisième élément

1. un
2. deux

> Une citation sur
> deux lignes.

## Code

```python
def hello(name):
    return f"Bonjour {name}"
```

## Tableau

| Colonne | Valeur | Remarque |
|---|---|---|
| a | 1 | premier |
| b | 2 | second |

## Diagramme

```mermaid
flowchart LR
    A[Début] --> B{OK ?}
    B -->|Oui| C[Fini]
    B -->|Non| A
```

---

""" + "\n\n".join(f"### Section {i}\n\n" + "Du texte qui remplit la page. " * 30 for i in range(1, 9))


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "spike.pdf")
    options = dict(
        cover=dict(subtitle="Version 1.0", author="Joachim", date="19 novembre 2026"),
        toc=True,
        header="{title}",
        base_dir=str(out.parent),
    )
    if len(sys.argv) > 2:
        options["theme"] = sys.argv[2]
    pyegui.markdown_to_pdf(TEXT, str(out), **options)
    doc = pyegui.PdfDocument(str(out))
    print("opened:", doc.pages, "pages, error:", doc.error)
    print("outline:", doc.outline)
    print("text of page 1:", repr(doc.text(1)[:300]))
    for page in range(1, doc.pages + 1):
        out.with_suffix(f".p{page}.png").write_bytes(doc.render_png(page, width=900))


if __name__ == "__main__":
    main()
