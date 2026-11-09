import base64
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import padding

PUB_B64 = ("MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEApRvT6vZtFVIycWoJ"
"fecUiYgw1o/ph9vFrfMsW3nUxpCHbDZDBJW3Lwf2oGJ2gNE4Vu2jhs+FGCq/ClU4XaOCt15gVU+zz"
"L9X0fKDrAYjTwk8mHIygLrSOaTbJGPLIBrqjYMJMZIfvlgyvyKoT24PkxuAFpFWZLKPMxvXbaq+25"
"dePWl1htlZlyMBfFOGPRKoz7+HTMilpeXqiEDFl/4k/wion4W6yUw7vt3TGBtffhqbYuWY89PmHas"
"5ITFaHMQR9t4EOJRddDuQcHkqnEEHhlxvOBsT+i9ITjFzcBMFyh7Wj3w1EIVcYgYbIIZvbujgqtmO"
"+yCLDszTP8JQ3NYEJwIDAQAB")

def custom(v):
    pem = b"-----BEGIN PUBLIC KEY-----\n" + b"\n".join(PUB_B64[i:i+64].encode() for i in range(0,len(PUB_B64),64)) + b"\n-----END PUBLIC KEY-----\n"
    key = serialization.load_pem_public_key(pem)
    return base64.b64encode(base64.b64encode(key.encrypt(v.encode(), padding.PKCS1v15()))).decode()

# 关键请求头： Cookie: j=<ts> 且 Header: j=<ts>（同值）
