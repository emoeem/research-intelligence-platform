#!/usr/bin/env python3
# token = base64( AES-128-ECB( bare_token_32hex, key ) )
# key   = md5( KM + '==' + rand3 + SECRET[idx+KM] + '-' + rand3 + ',' + TS16 + '+' + KM ).substr(KM,16)
# TS16  = 13位毫秒 + 3位随机数 (即 timestamp 头原值) ; idx = ((UTC小时+8)%24)%12 ; KM = keyMove
import base64, hashlib, time, os
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives import padding

KEYMOVE = 3
DECRYPT_SECRET = ["UgxfbCkc_D~3V","f&rWCozIBl*xW","w*8A0NX6fHPCJ","=1y*iQ6%Dr34^",
"TXYTO!GNMln9z","ntbJ1I9^Ykhnd","0iLMWyxB3d^E5","%^n1vJ%fzUHSv","k!xgy9SYiNX!a",
"OASlI6Ln=xII#","TDY@VP3o7rK8e","Gl!AGr7Zcu#Ab","3ldydgJI%~oh=","jT*olrzx9qlAd",
"I^g4USD8PRic*","5l9ym2ck*ALq8","eGuL6@@=#NXE3","6Pd8LkO*sF^2I","MUKk7&sX_g0@F",
"CjavE!dcqUY%y","q^jFM4Tf2KGda","5e!v3Q~tvDZdv","O4To*5xTX=G82","QN945YmVfYHKh",
"rY83f^tMYRio!","dKf~V8Aa6G#xy","a7srSqRD8J@7t","r98Qy%BxZ3RoY","aG%Epo9le8yH@",
"A^c8WlVcaGU72"]

def idx_for(ts_ms): return ((((ts_ms//1000//3600)%24)+8)%24)%12

def key_for(ts16):
    ts_ms=int(ts16[:13]); r3=ts16[13:16]
    sec=DECRYPT_SECRET[idx_for(ts_ms)+KEYMOVE]
    return hashlib.md5(f"{KEYMOVE}=={r3}{sec}-{r3},{ts16}+{KEYMOVE}".encode()).hexdigest()[KEYMOVE:KEYMOVE+16]

def mint(bare, ts16=None):
    if ts16 is None:
        ms=int(time.time()*1000); ts16=f"{ms}{int.from_bytes(os.urandom(2),'big')%1000:03d}"
    k=key_for(ts16)
    p=padding.PKCS7(128).padder(); d=p.update(bare.encode())+p.finalize()
    e=Cipher(algorithms.AES(k.encode()),modes.ECB()).encryptor()
    return base64.b64encode(e.update(d)+e.finalize()).decode(), ts16

def unmint(tok, ts16):
    k=key_for(ts16)
    d=Cipher(algorithms.AES(k.encode()),modes.ECB()).decryptor()
    pt=d.update(base64.b64decode(tok))+d.finalize()
    u=padding.PKCS7(128).unpadder()
    return (u.update(pt)+u.finalize()).decode()

if __name__=="__main__":
    import sys
    print(mint(sys.argv[1]) if len(sys.argv)>1 else "usage: mint.py <bare32hex>")
