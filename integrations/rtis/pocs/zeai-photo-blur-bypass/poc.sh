# 1) 从任意公开主页取模糊图文件名
curl -s https://TARGET/u/UID | grep -oE 'https://[^"]+/up/p/[^"]+_blur\.jpg' | head -1
# 2) 将 _blur 替换为 _b，直接下载原图（无需登录）
curl -s -o orig.jpg "https://TARGET/up/p/m/YYYY/MM/UID_TS_b.jpg"
# 3) 判断是否清晰：与原 _blur 图比 Laplacian variance / 文件尺寸
python3 - <<'PY'
from PIL import Image
import numpy as np
from scipy import ndimage
for p in ['orig.jpg','blur.jpg']:
    a=np.asarray(Image.open(p).convert('L'),dtype=float)
    print(p, Image.open(p).size, round(float(ndimage.laplace(a).var()),1))
PY
