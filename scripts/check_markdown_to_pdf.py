"""Checks of markdown_to_pdf (options, errors, thread, odd markdown): python scripts/check_markdown_to_pdf.py"""

import sys
import tempfile

import pyegui

MD = "# Titre\n\nUn [lien](https://example.org) et du texte.\n\n## Sous-titre\n\n" + "Texte. " * 400 + "\n\n```python\nprint('x')\n```\n"
ok = True


def check(name, cond, detail=""):
    global ok
    ok &= bool(cond)
    print(("PASS " if cond else "FAIL ") + name, detail)


# bytes when no path
data = pyegui.markdown_to_pdf(MD)
check("returns bytes", isinstance(data, bytes) and data.startswith(b"%PDF"), len(data))

# empty text
data = pyegui.markdown_to_pdf("")
check("empty text gives a PDF", data.startswith(b"%PDF"))

# options
for kwargs in [
    dict(page_size="Letter"),
    dict(page_size=(100, 150), margin=8),
    dict(theme="print", footer=False, header=dict(left="{title}", right="{page}")),
    dict(toc=dict(title="Contents", depth=1), cover=True, bookmarks=False),
    dict(footer="{title} - {page}/{pages}", title="Mon titre", author="Moi"),
]:
    try:
        d = pyegui.markdown_to_pdf(MD, **kwargs)
        check(f"options {list(kwargs)}", d.startswith(b"%PDF"))
    except Exception as e:
        check(f"options {list(kwargs)}", False, repr(e))

# errors
for kwargs, exc in [
    (dict(theme="blue"), ValueError),
    (dict(page_size="B7"), ValueError),
    (dict(margin=500), ValueError),
    (dict(cover="x"), ValueError),
    (dict(toc=3.5), ValueError),
]:
    try:
        pyegui.markdown_to_pdf(MD, **kwargs)
        check(f"raises {kwargs}", False, "no error")
    except exc as e:
        check(f"raises {kwargs}", True, str(e)[:60])
    except Exception as e:
        check(f"raises {kwargs}", False, repr(e))

# a heading with formatting, an unclosed mermaid and a broken diagram must not crash
tricky = "# **Gras** et `code`\n\n```mermaid\ngraph TD\n A-->\n```\n\n```mermaid\nnot a diagram at all\n```\n\n## Fin\n"
d = pyegui.markdown_to_pdf(tricky, toc=True)
check("tricky markdown", d.startswith(b"%PDF"))
folder = sys.argv[1] if len(sys.argv) > 1 else tempfile.gettempdir()
doc_path = folder + "/pyegui_tricky.pdf"
open(doc_path, "wb").write(d)
doc = pyegui.PdfDocument(doc_path)
check("tricky outline", [o["title"] for o in doc.outline] == ["Gras et code", "Fin"], [o["title"] for o in doc.outline])

# thread
import threading

res = []
t = threading.Thread(target=lambda: res.append(pyegui.markdown_to_pdf(MD, toc=True)))
t.start()
t.join()
check("works in a thread", res and res[0].startswith(b"%PDF"))

print("ALL OK" if ok else "SOME FAILED")
sys.exit(0 if ok else 1)
