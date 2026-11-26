# POC 证据恢复矩阵（2026-11-26）

本报告只核对 Overlay 中的路径、Commit 与 Evidence Ref 元数据，不读取 POC 正文，不执行 POC，不连接目标系统。文件存在只证明来源可定位，不代表其中包含完整、有效或可独立复现的证据。

## 汇总

| 指标 | 当前结果 |
|---|---:|
| 证据缺口记录 | 33 |
| 缺失证据项目 | 94 |
| 具有本地 source_path 且路径存在 | 33/33 |
| 具有 Evidence Ref | 25/33 |
| Evidence Ref 总数 | 51 |
| Evidence Ref 实际路径可解析 | 51/51 |
| 具有可解析 source_commit | 30/33 |

当前 33 条证据缺口记录中，33 条本地来源路径存在；ID 16 的源文件已按同目录唯一脚本恢复（历史数据库路径 basename 为 poc.sh，当前文件名为 poc-actuator.sh），但这只恢复 source traceability，不补足 raw response 或 impact boundary。25 条至少有一个 Evidence Ref，8 条没有 Evidence Ref。现有 51 个 Evidence Ref 的路径全部可解析。上述 51 个引用包含脚本、说明和历史报告等不同类型，不能等同于 51 份原始请求/响应或独立复现证据。

没有找到可安全确认为这些缺口对应的额外权威运行产物，因此没有自动填入猜测性 Evidence Ref，也没有提升 K 级或 Lifecycle。

## 逐条矩阵

| Knowledge ID | 缺失证据 | source_path | Evidence Ref | Commit |
|---:|---|---|---:|---|
| 4 | `raw_request_response`, `negative_control`, `independent_reproduction` | 存在：`integrations/rtis/pocs/列表接口-limit-page-直通探测-判定-服务端未裁剪-前端过滤-型付费墙绕过/limit_escalation_probe.sh` | 无 | 已解析 |
| 10 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/世纪佳缘-花千树-_t-签名离线伪造-cpi.jiayuan.com-fam-未授权访问` | 2 个（均可解析） | 已解析 |
| 12 | `protected_route_response`, `cross_tenant_response`, `negative_control` | 存在：`integrations/rtis/pocs/多租户-saas-鉴权前缀差分法-401-9999-404-三态判读-定位拦截器覆盖缺口/saas-prefix-diff.sh` | 无 | 已解析 |
| 16 | `raw_request_response`, `impact_boundary` | 存在：`integrations/rtis/pocs/n-a-configuration-exposure-spring-cloud-alibaba-actuator-泄露-nacos-凭据-绕过-show-val/poc-actuator.sh`（原 DB basename 为 `poc.sh`） | 无 | 已解析（RTIS 迁移提交，非测试执行 commit） |
| 18 | `openapi_json`, `raw_request_response`, `auth_control` | 存在：`integrations/rtis/pocs/springdoc-v3-api-docs-白名单绕过-零凭据拉取微服务全量-openapi-sa-token-场景/springdoc-whitelist-enum.sh` | 无 | 已解析 |
| 19 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/spring-cloud-gateway-springdoc-api-wg-svc-v3-api-docs-零认证拿全量接口契约` | 2 个（均可解析） | 已解析 |
| 20 | `oracle_calibration_response`, `dictionary_run_log` | 存在：`integrations/rtis/pocs/spring-boot-统一响应体三值-oracle-anon-handler-存在性枚举器/anon_oracle_enum.py` | 无 | 已解析 |
| 27 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/认证后不校验角色-会员-token-访问代理端-api-agent-接口-code-400-业务错-401-鉴权错` | 2 个（均可解析） | 已解析 |
| 29 | `dated_response_matrix`, `target_path_mapping`, `run_log` | 存在：`integrations/rtis/pocs/鉴权实现四态判定探针-区分-fail-open-存在性校验-值校验-无鉴权/auth4state.sh` | 无 | 已解析 |
| 31 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/包网-h5-api-签名仅校验非空判定-会话-token-跨路径复用-成对重放` | 1 个（均可解析） | 已解析 |
| 35 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/包网-app_config.txt-离线解密-aes-256-cbc-sha256-硬编码常量-iv-全-0` | 3 个（均可解析） | 已解析 |
| 36 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/包网-miwen-请求签名-secret-派生-js-base64-容错吃掉注入垃圾` | 3 个（均可解析） | 已解析 |
| 40 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/多租户-saas-的-ssrf-白名单字典-schema-存在性-oracle-yz_sys_dict-error-1146-按租户取-token` | 2 个（均可解析） | 已解析 |
| 43 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/bbin-系包网-entrance-api-加密通道解密算法-md5-alias-aes-256-ecb-双层-base64` | 3 个（均可解析） | 已解析 |
| 45 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/代理后台-bundle-硬编码-aes-256-gcm-密钥-匿名解密全品牌-ag-hall-远程配置` | 3 个（均可解析） | 已解析 |
| 48 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/包网-h5-白牌系-token-派生算法-aes-128-ecb-md5-动态密钥-离线铸造器` | 3 个（均可解析） | 已解析 |
| 56 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/前置分发器前缀白名单绕过-entrance-..-直连裸源站-cdn-绕过` | 1 个（均可解析） | 已解析 |
| 60 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/包网-bbin-系-registeragent-addagent.json-代理在线注册参数契约-account-首字母-长度-规则的两维业务码盲反推法` | 3 个（均可解析） | 已解析 |
| 66 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/spa-接口加密通道反推与-encrypt_query-解密-伪造-aes-128-cbc-未授权下发静态-key-iv` | 3 个（均可解析） | 已解析 |
| 72 | `raw_request_response`, `endpoint_identity`, `current_key_rotation_status` | 存在：`integrations/rtis/pocs/com.mltech.sign-签名密钥下发接口未授权利用-自传公钥换回-hmac-key-免签路径清单-rsa-oaep-sha256-aes-256-gcm/yidui_signkey.py` | 无 | 已解析 |
| 83 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/小程序-wxapkg-内-cryptojs-硬编码-aes-密钥定位与字段级-pii-解密-含重加密复现判据` | 3 个（均可解析） | 已解析 |
| 59648 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/yc-站群落地页-_c-参数-js-注入-点击后任意域跳转-hex2bin-模板` | 1 个（均可解析） | 已解析 |
| 59649 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/落地页-_s-参数注入-零点击任意域跳转-绕-黑名单` | 1 个（均可解析） | 已解析 |
| 59650 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/落地页-_s-参数未转义注入-零点击任意跳转-hex2bin-模板族-绕过-黑名单` | 1 个（均可解析） | 已解析 |
| 59651 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/x-forwarded-for-注释逃逸-文档首部任意-js-执行-落地页站群-开放重定向原语` | 1 个（均可解析） | 已解析 |
| 59655 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/spring-boot-微服务零鉴权全链路验证法-空-body-判别-错误文案反推参数-加密响应-size-oracle` | 1 个（均可解析） | 已解析 |
| 900009 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/pig4cloud-包网-oss-与-hall-api-双层加密通道密钥还原-aes-ecb-thankspig4cloud-aes-cbc-md5token` | 1 个（均可解析） | 已解析 |
| 900033 | `raw_challenge_response`, `authorized_sms_outcome`, `cross_session_replay`, `negative_control` | 存在：`integrations/rtis/pocs/zeai-婚恋-cms-图形点选验证码可程序化绕过-挑战与答案同源下发-短信通道对脚本开放` | 1 个（均可解析） | 已解析 |
| 900043 | `current_state_regression`, `structured_outcome_ref` | 存在：`integrations/rtis/pocs/明文身份-cookie-伪造-user_id-user_time-无签名即完成鉴权-thinkphp-后台任意用户接管/cookie_identity_forgery.md` | 4 个（均可解析） | 已解析 |
| 900047 | `matching_frontend_bundle`, `construction_function`, `forged_request_response` | 存在：`integrations/rtis/pocs/rule-zero-cred-session-forge-frontend-crypto/poc.txt` | 无 | 已解析 |
| 900061 | `structured_outcome_ref` | 存在：`integrations/rtis/pocs/恋恋系-xingqyh-icu-充值下单-rechargeAmount-与-itemId-解耦-金额校验缺失实测/request.http` | 2 个（均可解析） | 无可解析 Commit |
| 900062 | `arbitrary_userid_second_account_response`, `negative_control`, `structured_outcome_ref` | 存在：`integrations/rtis/pocs/恋恋系-xingqyh-icu-H5-签名盐公开-guest-代充接口任意-userId-建单/request.http` | 2 个（均可解析） | 无可解析 Commit |
| 900063 | `cross_account_validation`, `structured_outcome_ref` | 存在：`integrations/rtis/pocs/恋恋系-xingqyh-icu-免费充值路径反知识-入账面防护实测清单/request.http` | 2 个（均可解析） | 无可解析 Commit |

## 明确未解决项

- **ID 16**：源脚本已从同目录唯一的 poc-actuator.sh 恢复，source_commit 指向包含该文件的 RTIS 迁移提交；这不是测试执行 commit。仍缺 raw_request_response 与 impact_boundary，继续保持 K0/Candidate，不得推断 Nacos 可达或下游数据可访问。
- **ID 4、12、18、20、29、72、900047**：本地源文件存在，但没有独立 Evidence Ref。源码/脚本不能替代原始请求响应、对照和独立复现。
- **ID 900043**：已有历史材料引用，但仍缺当前状态回归和结构化 Outcome Ref。
- **ID 900061–900063**：已有本地 request / 历史报告引用，但仍缺结构化 Outcome Ref、跨账号/阴性对照等指定证据；源文件没有可解析 Git Commit。
- **ID 900033**：缺原始挑战响应、授权 SMS 结果、跨会话回放和阴性对照。本轮不执行真实短信发送或真实账户流程；只有在授权隔离环境的合成 SMS sink 中才能补充相应验证。
- 其余 K0/Candidate 记录仍需原始请求响应、阴性对照和独立复现。若目标已修复、下线或无法复现，应记录历史不可复现，不得伪造成功结果。

## 来源追溯结论

- Overlay 全量仍为 49 条：42 条有本地来源路径、39 条有可解析 Commit、9 条记录上游来源 URL。
- 7 条无本地 source_path：均为工具/方法参考且已有上游 URL。
- 10 条无 Commit：上述 7 条无本地路径记录，加上 ID 900061–900063 的本地源文件没有 Git 历史。
- 不为缺失路径、Commit 或证据生成虚构值。

## 范围边界

- 不访问未经授权的公网目标，不重放真实账户、短信或支付流程。
- 不执行敏感信息审计、凭据扫描或脱敏工作；不将原始 POC 正文复制到新报告。
- 不修改 RTIS 原始生产数据库或原始 POC；治理数据继续留在 RIP Overlay。
- 证据缺失保持显式状态；只有获得真实、可追溯的材料后才能解除相应 `missing_evidence`。


## 补充历史运行目录核对（2026-11-26）

本轮只检查了目录名、文件名、文件大小/时间戳，以及 POC 元数据中的路径引用标记；没有读取这些候选目录的响应正文、脚本正文或会话文件。

| Knowledge ID | 候选目录 | 文件清单观察 | 来源关联结论 |
|---:|---|---|---|
| 4 | 本地 POC 目录 | 仅 NOTES.md 与 limit_escalation_probe.sh | 没有原始请求/响应或独立运行产物 |
| 12 | 本地 POC 目录；另有 ../runs/jxnu/pan | POC 目录仅 NOTES.md 与 saas-prefix-diff.sh；旁路目录存在 prefix_probe.txt、prefix.py 等 | POC 的 verified_note 不含该运行目录引用，k_event 无对应事件；不能证明是同一目标/同一测试 |
| 18 | 本地 POC 目录 | 仅 NOTES.md 与 springdoc-whitelist-enum.sh | 没有 OpenAPI 原始响应或 auth control 产物 |
| 20 | 本地 POC 目录；另有 ../runs/aozoneyun-admin/pwd-oracle | POC 目录含 anon_oracle_enum.py、poc.sh；旁路目录含 ORACLE.md、HTTP header/body 文件及 phpsessid.txt | POC 的 verified_note 不含该运行目录引用，k_event 无对应事件；名称相似不足以建立关联。phpsessid.txt 按潜在会话敏感材料处理，未读取、复制或引用 |
| 29 | 本地 POC 目录 | 仅 auth4state.sh 与 poc.sh | 没有 dated response matrix 或 run log |
| 72 | 本地 POC 目录 | 仅 NOTES.md、yidui_signkey.py 与 poc.sh | 没有原始响应或当前密钥轮换状态材料 |
| 900047 | 本地 POC 目录 | 仅 poc.txt | 没有匹配的前端 bundle、构造函数定位记录或伪造请求响应 |

额外来源追溯：

- ID 4、12、18、20、29、72、900047 当前 Overlay 的 source_commit 都是 022a71718740a7fad74ca194a10e9cc9269501c4，该提交是 2026-11-09 的 RTIS 目录迁移提交，不是对应测试的执行 commit。
- 对 ID 12 与 ID 20，已对原始 RTIS DB 的 verified_note 做精确路径存在性检查，并核对 k_event 元数据；没有发现候选 runs 目录的引用或对应运行事件。
- 因此没有把 ../runs/jxnu/pan 或 ../runs/aozoneyun-admin/pwd-oracle 添加为 Evidence Ref；8 条无 Evidence Ref 记录数不变。K 级与 Lifecycle 均保持不变；仅 ID 16 因源脚本恢复而移除 source_artifact 一项，raw_request_response 与 impact_boundary 仍缺失。
