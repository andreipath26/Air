"""Shared bounded answer contract. Extract without consulting expected answers."""
import json,unicodedata,re
def extract(raw):
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
    if text.casefold() in {"unknown","null","i don't know","uncertain","n/a"}:return None,"abstained"
    if not text or len(text)>256 or len(text.split())>12 or any(ord(c)<32 or c in "{}[]?!" for c in text):return None,"malformed"
    return text,"extracted"
def normalize(value,group):
    if value is None:return None
    value=" ".join(unicodedata.normalize("NFC",value).split())
    if group in ("science","engineering"):
        # Greek mu and micro sign are interchangeable in this narrow notation contract.
        return value.replace("μ","µ")
    return value.casefold()
def matches(actual,expected,group):
    return normalize(actual,group)==normalize(expected,group)
