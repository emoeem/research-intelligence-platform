# M8 收尾：Embedding 真实模型后端接入报告

日期：2026-11-07　执行：RIP worker（主会话派工）　范围：`workers/python/embedding_worker.py` + 测试 + 本报告；关闭 M8_READINESS 第三节限制 4（"Worker embedding 为确定性占位，真实模型接入留 VectorModel 接口"）。

## 一、结论

embedding worker 从"256 维确定性 hash 占位"升级为"占位 + 真实模型双后端"：

- 新增 `bge-small-zh-v1.5` ONNX 后端：onnxruntime CPU 推理 + tokenizers fast tokenizer，512 维 L2 归一化语义向量，中文语料主用。
- 协议向后兼容：默认模型仍是 `hashing-ngram-v1`（256 维，零依赖），旧请求（不带 `model`）行为不变，响应仅新增 `backend` 字段。
- 模型/运行时不可用时自动回退 hash 后端，响应 `backend="hash-fallback"` 标注，stderr 有 warn 事件留痕。
- 测试：基线 109 全过不变；新增 16 用例后 125 全过（有模型环境）；CI 口径（无 onnx 依赖裸环境）125 中 11 skip 其余全过，skipUnless 守卫生效。

## 二、选型与理由

选定 **BAAI/bge-small-zh-v1.5 的 ONNX 导出版**（onnx-community 导出，ModelScope 镜像）。

理由：

1. 语料中文为主：bge-small-zh-v1.5 是中文检索/相似度任务的主流小模型（24M 参数，hidden=512），MTEB 中文语义检索同类小模型中质量优先。实测相似句对余弦 0.838，不相似句对 0.179，区分度显著。
2. ONNX 可得性实测：本机 GitHub 仅 api/codeload 可达，HuggingFace 直接拉取不可靠；ModelScope（modelscope.cn）可达且有现成导出：`onnx-community/bge-small-zh-v1.5-ONNX`（onnx-community 为 HF 官方 ONNX 社区组织导出，带 fp32/fp16/q4/quantized 多档与 tokenizer.json）。ModelScope 搜索接口另见 6 个第三方 onnx 导出仓库（lyf219/AlexZh/phodit 等），其中 AlexZh 仓库只有 vocab.txt 无 tokenizer.json，需要额外拼装，故选 onnx-community。
3. 体积：fp32 `model.onnx` + `model.onnx_data` 共 94.8MB（外加权重外置的 onnx 格式），远低于 1GB 红线。fp16（45MB）与 quantized（22MB）以后需要时可按同目录校验和方式补拉，接口不变。
4. 依赖轻：onnxruntime + tokenizers + numpy 三个包，无 torch，无 sentence-transformers。
5. 备选对比：all-MiniLM-L6-v2 onnx 中文效果弱（主要为英文训练），distiluse-base-multilingual-cased 中文表现也不如 bge-small-zh，均未采用。

## 三、模型来源与校验和

来源仓库：`https://modelscope.cn/models/onnx-community/bge-small-zh-v1.5-ONNX`（revision master）。
落盘目录：`models/bge-small-zh-v1.5/`（gitignore，不入库）。

| 文件 | 大小 | sha256（与 ModelScope API 元数据逐字节一致） |
|---|---|---|
| model.onnx | 41,689 B | 69b353bb2aa2d09ab606ddbbc35437b03c843615a6bff28216a37fee7309c2aa |
| model.onnx_data | 94,765,056 B | e72da961b03613124aa11317470c995ca197651a9d7f6be2b0e90aad92f71df0 |
| tokenizer.json | 362,603 B | 3d09c84ebd10306706a79a8276b3ab736a40d8ec03251c7639f4e52c3a1a4f8e |
| config.json | 904 B | 34fa1ea6278c257de3cc8ce7e9bdc48647b802145a9da0fc32e95db620efd04f |

重建命令（克隆后）：

```bash
mkdir -p models/bge-small-zh-v1.5 && cd models/bge-small-zh-v1.5
base="https://modelscope.cn/models/onnx-community/bge-small-zh-v1.5-ONNX/resolve/master"
for f in onnx/model.onnx onnx/model.onnx_data tokenizer.json config.json; do
  curl -L -o "$(basename $f)" "$base/$f"
done
sha256sum model.onnx model.onnx_data tokenizer.json config.json   # 与上表核对
```

依赖安装（worker 专用 venv，不入库）：

```bash
cd workers/python && python3 -m venv .venv
./.venv/bin/pip install onnxruntime==1.29.0 tokenizers==0.23.3 numpy==2.5.4
```

版本即本机实测版本（Python 3.14.9）。

## 四、实现要点（workers/python/embedding_worker.py）

- `BgeOnnxModel(VectorModel)` 实现 M3 留下的 `VectorModel` 接口并在 `MODEL_REGISTRY` 注册，协议层 budget/幂等/取消零改动。onnxruntime/tokenizers 延迟导入（仅在构造该模型时 import），模块本身保持零第三方依赖可导入。
- 输出解析：优先取导出图的 `sentence_embedding` 输出（CLS pooling + L2 归一化已固化在图内，实测归一化范数=1.0）；若导出只有 `last_hidden_state` 则取 [CLS] 位并自行 L2 归一化，兼容不同导出图。feeds 按模型声明的输入集合过滤（input_ids/attention_mask/token_type_ids）。
- 确定性三件套：
  1. 逐条推理（batch=1），padding 不进图，结果不依赖批内其他文本长度（有测试断言批内向量与单条严格一致）；
  2. SessionOptions 固定 `intra_op_num_threads=4 / inter_op=1`，同机同输入严格同输出；
  3. tokenizer 截断 max_length=512 token，超长文本确定性截断（有测试）。
- 会话缓存：模块级 `_MODEL_CACHE`（key=模型目录）复用 InferenceSession，避免每请求重复加载 95MB；onnxruntime `Session.run` 线程安全，加载在锁外不阻塞并发请求。
- 实测性能（本机）：单条短句含冷启动 0.25s；64 条中文短句批量 437ms，响应行 345KB（默认 1MiB budget 内）。
- `worker_version` 0.1.0 -> 0.2.0：模型升级走版本递增使旧幂等缓存失效（缓存指纹含 worker_version）。

## 五、维度处理

约束排查结论：**Rust 侧与库 schema 均未硬编码 embedding 维度**。全仓 grep crates/、apps/、schemas/、migrations/*.sql：无 embeddings 表（M4 报告已注明"向量面无数据源"）、search-engine 仅为 FTS + 混合检索、无向量存储代码。维度约束只存在于 worker 协议层（旧代码硬编码 dim=256）与协议 schema 的描述文本。

因此维度方案落在 worker 层做显式协商，不做迁移：

- 每个模型声明自己的输出维度（hash=256，bge=512），响应 `dim` 字段回显真实维度。
- `task.dim` 缺省：接受所选后端输出维度；显式给出时必须等于该后端维度，否则 -32602（`data.requested` / `data.supported` 说明），无静默截断。
- 回退路径的维度冲突显式报错：请求真实模型但不可用、且显式 `dim=512` 与回退维度 256 冲突时，返回 -32602 "refusing silent-dim fallback"，绝不静默交付错维向量（有测试）。
- `schemas/worker-protocol/v1.json` 的 task 描述同步更新（model 取值、backend 字段、dim 协商语义）。
- bge 的 `ngram_range` 参数显式拒绝（-32602），该参数仅属于 hash 后端。

## 六、回退语义

`backend` 字段取值：

| 取值 | 含义 |
|---|---|
| `hash` | 按请求（含默认）使用 hash 后端，未发生回退 |
| `onnx:bge-small-zh-v1.5` | ONNX 真实模型后端正常工作 |
| `hash-fallback` | 请求了真实模型但不可用，回退 hash |

触发回退的情形（一律 stderr `embedding_backend_fallback` warn 事件留痕，含原因）：

1. 依赖缺失（CI 无 onnxruntime/tokenizers：ImportError 分支）；
2. 模型文件缺失/路径错误（`EMBEDDING_MODEL_DIR` 指错：FileNotFoundError 分支）；
3. 模型加载失败（onnx 图不合法等，任意异常分支）；
4. 显式开关 `EMBEDDING_BACKEND=hash` 强制回退（CI/排障用）。

环境变量：`EMBEDDING_MODEL_DIR`（模型目录，需含 model.onnx 与 tokenizer.json，缺省仓库根 `models/bge-small-zh-v1.5/`）、`EMBEDDING_BACKEND=hash`（强制 hash）。

回退后响应仍含 `ngram_range` 字段（hash 语义保持），`deterministic=true` 不变（hash 与 onnx 两后端各自确定）。

已知边界：幂等缓存指纹=（worker_version, task），不含后端可用性。同一台机器上真实模型由"可用"变"不可用"后再以相同幂等键重放，会命中先前缓存结果。跨机器缓存目录不共享，本机重建缓存目录即消除；影响仅限命中旧后端结果，不改协议。

## 七、测试结果

| 口径 | 命令 | 结果 |
|---|---|---|
| 基线（改动前） | `python3 workers/python/tests/run_all.py` | 109 过 0 败 0 skip |
| 改动后 + 有模型（venv） | `workers/python/.venv/bin/python tests/run_all.py` | **125 过，0 败，0 skip** |
| CI 口径（无 onnx 依赖裸 venv，模型存在但依赖缺失） | `python3 -m venv --without-pip /tmp/ci-sim-venv` 后用其 python 跑 run_all.py | **125 中 11 skip（模型用例 skipUnless 生效），114 过，0 败** |

新增 16 用例（`workers/python/tests/test_embedding_model.py`）：

- 进程内 `BgeOnnxModel` 单元 6 个：dim=512、确定性（同输入同输出逐值相等）、L2 归一化、空输入不崩且归一化、中文相似句 0.838 > 不相似句 0.179（断言差距 > 0.1）、超长文本 512 token 截断。
- 协议层 10 个：hash 默认响应新增 backend 字段（向后兼容）、onnx 端到端（backend/onnx:bge-small-zh-v1.5、dim=512、两次调用向量逐值相等）、协议层中文语义排序、批内向量与单条严格一致、dim=512 收 dim=256 拒、bge+ngram_range 拒、指错模型目录强制回退（hash-fallback、256 维、仍确定性）、`EMBEDDING_BACKEND=hash` 强制回退、回退+dim=512 显式冲突 -32602、worker_version=0.2.0。

回退逻辑可被强制触发已验证：CI 口径下走 ImportError 分支、本机指错 `EMBEDDING_MODEL_DIR` 走 FileNotFoundError 分支，均产出 `backend="hash-fallback"`。

## 八、遗留

1. 幂等缓存指纹不含后端可用性（见第六节已知边界），如需严格一致可在指纹中加入 backend 探测结果，代价是每次请求探测。
2. reranking worker 仍未实现（M3 报告遗留 9），其前提（真实 embedding）已就位。
3. Rust 侧向量检索面（embeddings 表、混合检索向量路）依旧未实现，属 M4 已归因缺口，不在本任务范围。
4. 模型目录不入库；换机器需按第三节命令重建。fp16/quantized 变体未拉取，需要时同源同法补拉。

## 九、验证清单（完成前自查）

- [x] 本地 python workers 全部测试通过（venv 口径 125/125）。
- [x] CI 口径（无 onnxruntime 环境）unittest discover 跑通，11 skip 生效，0 败。
- [x] 有模型环境真实后端产出向量（onnx:bge-small-zh-v1.5，512 维）且回退逻辑可强制触发（错路径 + env 开关两路）。
- [x] 报告数字与实测一致（相似度、延迟、测试计数均为本次实测值）。
- [x] 模型 sha256 与 ModelScope API 元数据逐字节核对一致。
