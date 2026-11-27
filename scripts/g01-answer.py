"""Shared bounded answer contract. Extract without consulting expected answers."""
import json,unicodedata,re
def extract(raw,subject=None,relation=None):
    if not isinstance(raw,str) or len(raw)>16384:return None,"malformed"
    text=raw.strip()
    fence=chr(96)*3
    if text.startswith(fence) and text.endswith(fence):
        parts=text.split("\n",1)
        if len(parts)!=2:return None,"malformed"
        text=parts[1][:-3].strip()
    if not text:return None,"malformed"
    if text.startswith("{"):
        try:
            obj=json.loads(text)
            if type(obj) is not dict or set(obj)!={"answer"}:return None,"malformed"
            if obj["answer"] is None:return None,"abstained"
            if not isinstance(obj["answer"],str):return None,"malformed"
            text=obj["answer"].strip()
        except (ValueError,TypeError):return None,"malformed"
    else:
        text=re.sub(r"^(?:answer\s*:\s*|the answer is\s+)","",text,flags=re.I)
    text=text.rstrip(".").strip()
    # Anchored, relation-specific forms only; never search for the expected answer.
    if subject and relation:
        s=re.escape(subject)
        rel=re.escape(relation)
        patterns=[rf"(?:the\s+)?{rel}\s+of\s+{s}\s+is\s+(.+)",rf"{s}(?:'s|’s)\s+{rel}(?:\s+city)?\s+is\s+(.+)",rf"{s}\s+has\s+(?:the\s+)?{rel}\s+(.+)",rf"{s}\s*\(([^()]+)\)"]
        for pattern in patterns:
            m=re.fullmatch(pattern,text,flags=re.I)
            if m:
                text=m.group(1).strip()
                break
        if " ".join(relation.casefold().split()) in {"si unit symbol","chemical symbol"}:
            # Small teachers often return a label with the symbol instead of
            # following the short-answer instruction. Extract only the
            # relation value; reference matching still decides acceptance.
            candidates=[
                rf"^{s}\s+(?:is\s+)?(?:represented|denoted|written|symboli[sz]ed)\s+(?:by\s+)?(?:the\s+)?(?:SI\s+)?(?:unit\s+)?symbol\s*(?:is\s+|:\s*)?(.+)$",
                rf"^(?:the\s+)?(?:SI\s+)?(?:unit\s+)?symbol\s*(?:is\s+|for\s+|of\s+|:\s*)?(.+)$",
                rf"^([A-ZµμΩ°]{{1,4}})\s+{s}$",
            ]
            for pattern in candidates:
                m=re.fullmatch(pattern,text,flags=re.I)
                if m:
                    text=m.group(1).strip()
                    break
            # Accept conventional “Name (symbol)” answers, even when the
            # model pluralizes the name or omits it entirely.
            m=re.search(r"\(([^()]{1,16})\)\s*$",text)
            if m:text=m.group(1).strip()
    if text.casefold() in {"unknown","null","i don't know","uncertain","n/a"}:return None,"abstained"
    if not text or len(text)>256 or len(text.split())>12 or any(ord(c)<32 or c in "{}[]?!" for c in text):return None,"malformed"
    return text,"extracted"
def normalize(value,group):
    if value is None:return None
    value=" ".join(unicodedata.normalize("NFC",value).split())
    if group in ("science","engineering"):
        # Greek mu and micro sign are interchangeable in this narrow notation contract.
        return value.replace("μ","µ")
    if group=="geography":
        # Place names are often written with or without diacritics in model
        # output (e.g. Bogotá/Bogota). Fold accents only for this natural-
        # language group; scientific notation remains exact.
        value="".join(c for c in unicodedata.normalize("NFD",value) if unicodedata.category(c)!="Mn")
    return value.casefold()
def matches(actual,expected,group):
    return normalize(actual,group)==normalize(expected,group)
