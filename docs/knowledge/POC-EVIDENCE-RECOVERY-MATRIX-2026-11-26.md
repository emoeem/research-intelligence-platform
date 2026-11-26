# POC 证据恢复矩阵（2026-11-26）

本报告只核对 Overlay 中的路径、Commit 与 Evidence Ref 元数据，不读取 POC 正文，不执行 POC，不连接目标系统。文件存在只证明来源可定位，不代表其中包含完整、有效或可独立复现的证据。

## 汇总

| 指标 | 当前结果 |
|---|---:|
| 证据缺口记录 | 33 |
| 缺失证据项目 | 95 |
| 具有本地 source_path 且路径存在 | 32/33 |
| 具有 Evidence Ref | 25/33 |
| Evidence Ref 总数 | 51 |
| Evidence Ref 实际路径可解析 | 51/51 |
| 具有可解析 source_commit | 29/33 |

当前 33 条证据缺口记录中，32 条本地来源路径存在，ID 16 的历史来源文件仍未定位；25 条至少有一个 Evidence Ref，8 条没有 Evidence Ref。现有 51 个 Evidence Ref 的路径全部可解析。上述 51 个引用包含脚本、说明和历史报告等不同类型，不能等同于 51 份原始请求/响应或独立复现证据。

没有找到可安全确认为这些缺口对应的额外权威运行产物，因此没有自动填入猜测性 Evidence Ref，也没有提升 K 级或 Lifecycle。

## 逐条矩阵

| Knowledge ID | 缺失证据 | source_path | Evidence Ref | Commit |
|---:|---|---|---:|---|
| 4 | `raw_request_response`, `negative_control`, `independent_reproduction` | 存在：`integrations/rtis/pocs/列表接口-limit-page-直通探测-判定-服务端未裁剪-前端过滤-型付费墙绕过/limit_escalation_probe.sh` | 无 | 已解析 |
| 10 | `raw_request_response`, `independent_reproduction`, `negative_control` | 存在：`integrations/rtis/pocs/世纪佳缘-花千树-_t-签名离线伪造-cpi.jiayuan.com-fam-未授权访问` | 2 个（均可解析） | 已解析 |
| 12 | `protected_route_response`, `cross_tenant_response`, `negative_control` | 存在：`integrations/rtis/pocs/多租户-saas-鉴权前缀差分法-401-9999-404-三态判读-定位拦截器覆盖缺口/saas-prefix-diff.sh` | 无 | 已解析 |
| 16 | `source_artifact`, `raw_request_response`, `impact_boundary` | 未定位 | 无 | 无可解析 Commit |
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

- **ID 16**：本地 source_path 与 source_commit 均缺失；目前没有权威历史来源文件。不得通过同名或相似漏洞文件猜测恢复。
- **ID 4、12、18、20、29、72、900047**：本地源文件存在，但没有独立 Evidence Ref。源码/脚本不能替代原始请求响应、对照和独立复现。
- **ID 900043**：已有历史材料引用，但仍缺当前状态回归和结构化 Outcome Ref。
- **ID 900061–900063**：已有本地 request / 历史报告引用，但仍缺结构化 Outcome Ref、跨账号/阴性对照等指定证据；源文件没有可解析 Git Commit。
- **ID 900033**：缺原始挑战响应、授权 SMS 结果、跨会话回放和阴性对照。本轮不执行真实短信发送或真实账户流程；只有在授权隔离环境的合成 SMS sink 中才能补充相应验证。
- 其余 K0/Candidate 记录仍需原始请求响应、阴性对照和独立复现。若目标已修复、下线或无法复现，应记录历史不可复现，不得伪造成功结果。

## 来源追溯结论

- Overlay 全量仍为 49 条：41 条有本地来源路径、38 条有可解析 Commit、9 条记录上游来源 URL。
- 8 条无本地 source_path：7 条工具/方法参考已有上游 URL，另 1 条为 ID 16 历史来源缺失。
- 11 条无 Commit：上述 8 条无本地路径记录，加上 ID 900061–900063 的本地源文件没有 Git 历史。
- 不为缺失路径、Commit 或证据生成虚构值。

## 范围边界

- 不访问未经授权的公网目标，不重放真实账户、短信或支付流程。
- 不执行敏感信息审计、凭据扫描或脱敏工作；不将原始 POC 正文复制到新报告。
- 不修改 RTIS 原始生产数据库或原始 POC；治理数据继续留在 RIP Overlay。
- 证据缺失保持显式状态；只有获得真实、可追溯的材料后才能解除相应 `missing_evidence`。
