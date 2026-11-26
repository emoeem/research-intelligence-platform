# CISA KEV × Nuclei 高风险漏洞清单（2026-11-26 快照）

本表按 CISA 收录日期排序，列出有已审核 Nuclei 模板覆盖的近期 KEV CVE。CISA KEV 表示已知在野利用，不代表某个目标当前仍受影响。模板只完成静态解析与来源审核，没有执行。

- CISA KEV：commit `51e0a777743fa06957afbaeb280f8735d292c1be`（CC0-1.0）；审核通过 1805 条。
- Nuclei：commit `a346d8f00c61bf95bbd30365f12cf923be51ba3d`（MIT）；审核通过 552 条匹配模板。
- 交叉覆盖：505 个 KEV CVE；共 552 条模板；当前展示 30 条。
- CVSS/EPSS 列取自 Nuclei 模板 classification，不是 CISA KEV 字段，也不是本工具独立请求 NVD/FIRST 后得到的最新评分。

| CVE | 厂商 / 产品 | CISA 收录 | Nuclei severity | Nuclei CVSS | Nuclei EPSS | 勒索软件使用 | 取证分诊 | CWE |
|---|---|---:|---|---:|---:|---|---|---|
| [CVE-2026-90817](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-90817) | Vanderbilt University / Research Electronic Data Capture (REDCap) | 2026-11-18 | critical | 9.8 | 0.33395 (98.4%) | Unknown | Yes | CWE-73, CWE-94 |
| ↳ 检测模板 | [CVE-2026-90817](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-90817.yaml) | | | | | | | |
| [CVE-2026-40217](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-40217) | BerriAI / LiteLLM | 2026-11-18 | high | 8.7 | 0.78004 (99.6%) | Unknown | No | CWE-420 |
| ↳ 检测模板 | [CVE-2026-40217](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-40217.yaml) | | | | | | | |
| [CVE-2026-109618](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-109618) | PHP Group / PHP | 2026-10-26 | critical | 9.2 | 0.99999 (100.0%) | Unknown | Yes | CWE-416, CWE-755, CWE-824, CWE-1325, CWE-1419 |
| ↳ 检测模板 | [CVE-2026-109618](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-109618.yaml) | | | | | | | |
| [CVE-2026-21589](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-21589) | Atlassian / Multiple Products | 2026-10-15 | critical | 9.8 | 0.90952 (99.8%) | Unknown | Yes | CWE-552 |
| ↳ 检测模板 | [CVE-2026-21589](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-21589.yaml) | | | | | | | |
| [CVE-2026-105133](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-105133) | Ahsay / AhsayCBS | 2026-10-14 | medium | 7.3 | 0.84478 (99.7%) | Unknown | No | CWE-287 |
| ↳ 检测模板 | [CVE-2026-105133](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-105133.yaml) | | | | | | | |
| [CVE-2026-104935](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-104935) | Ivanti / Connect Secure | 2026-10-13 | critical | 10.0 | 0.93096 (99.8%) | Unknown | Yes | CWE-290 |
| ↳ 检测模板 | [CVE-2026-104935](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-104935.yaml) | | | | | | | |
| [CVE-2015-3306](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2015-3306) | ProFTPD / ProFTPD | 2026-10-08 | critical | 10.0 | 0.99497 (99.9%) | Unknown | Yes | CWE-284 |
| ↳ 检测模板 | [CVE-2015-3306](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/network/cves/2015/CVE-2015-3306.yaml) | | | | | | | |
| [CVE-2016-3081](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2016-3081) | Apache / Struts | 2026-10-08 | high | 8.1 | 0.96916 (99.9%) | Unknown | Yes | CWE-77 |
| ↳ 检测模板 | [CVE-2016-3081](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2016/CVE-2016-3081.yaml) | | | | | | | |
| [CVE-2026-87902](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-87902) | WordPress / Core | 2026-09-25 | critical | 9.2 | 0.95769 (99.9%) | Unknown | Yes | CWE-98 |
| ↳ 检测模板 | [CVE-2026-87902](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-87902.yaml) | | | | | | | |
| [CVE-2026-71362](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-71362) | Adobe / Commerce and Magento | 2026-09-24 | critical | 9.1 | 0.91677 (99.8%) | Unknown | Yes | CWE-863 |
| ↳ 检测模板 | [CVE-2026-71362](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-71362.yaml) | | | | | | | |
| [CVE-2026-93616](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-93616) | Check Point / Multiple Products | 2026-09-22 | critical | 9.8 | 0.91605 (99.8%) | Unknown | Yes | CWE-22 |
| ↳ 检测模板 | [CVE-2026-93616](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-93616.yaml) | | | | | | | |
| [CVE-2026-76460](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-76460) | Cisco / Identity Services Engine | 2026-09-16 | critical | 10.0 | 0.75599 (99.5%) | Unknown | Yes | CWE-648 |
| ↳ 检测模板 | [CVE-2026-76460](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-76460.yaml) | | | | | | | |
| [CVE-2026-85706](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-85706) | GitLab / Community Edition and Enterprise Edition | 2026-09-11 | critical | 10.0 | 0.97355 (99.9%) | Unknown | Yes | CWE-35 |
| ↳ 检测模板 | [CVE-2026-85706](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-85706.yaml) | | | | | | | |
| [CVE-2026-42018](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-42018) | JFrog / Artifactory | 2026-09-11 | high | 7.5 | 0.09983 (95.7%) | Unknown | No | CWE-287 |
| ↳ 检测模板 | [CVE-2026-42018](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-42018.yaml) | | | | | | | |
| [CVE-2026-20079](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-20079) | Cisco / Secure Firewall Management Center (FMC) and Security Cloud Control (SCC) Firewall Management | 2026-09-09 | critical | 10.0 | 0.88658 (99.8%) | Unknown | Yes | CWE-288 |
| ↳ 检测模板 | [CVE-2026-20079](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-20079.yaml) | | | | | | | |
| [CVE-2026-86218](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-86218) | N-able / N-central | 2026-09-08 | critical | 10.0 | 0.15385 (96.9%) | Unknown | Yes | CWE-96 |
| ↳ 检测模板 | [CVE-2026-86218](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-86218.yaml) | | | | | | | |
| [CVE-2026-9586](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-9586) | Sangoma / Switchvox | 2026-09-02 | critical | 9.8 | 0.18979 (97.4%) | Unknown | Yes | CWE-89 |
| ↳ 检测模板 | [CVE-2026-9586](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-9586.yaml) | | | | | | | |
| [CVE-2026-83548](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-83548) | SonicWall / SMA1000 Appliances | 2026-09-02 | critical | 10.0 | 0.08043 (94.9%) | Unknown | Yes | CWE-918, CWE-441 |
| ↳ 检测模板 | [CVE-2026-83548](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/javascript/cves/2026/CVE-2026-83548.yaml) | | | | | | | |
| [CVE-2026-82329](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-82329) | JFrog / Artifactory | 2026-09-02 | critical | 9.8 | 0.14372 (96.7%) | Unknown | Yes | CWE-287 |
| ↳ 检测模板 | [CVE-2026-82329](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-82329.yaml) | | | | | | | |
| [CVE-2026-48710](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-48710) | Kludex / Starlette | 2026-09-02 | critical | 6.5 | 0.70288 (99.4%) | Unknown | No | CWE-444 |
| ↳ 检测模板 | [CVE-2026-48710](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-48710.yaml) | | | | | | | |
| [CVE-2026-81578](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-81578) | PaperCut / NG/MF | 2026-08-31 | critical | 9.8 | 0.84289 (99.7%) | Unknown | No | CWE-306 |
| ↳ 检测模板 | [CVE-2026-81578](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-81578.yaml) | | | | | | | |
| [CVE-2023-49105](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2023-49105) | ownCloud / ownCloud | 2026-08-27 | critical | 9.8 | 0.43190 (98.8%) | Unknown | Yes | CWE-287 |
| ↳ 检测模板 | [CVE-2023-49105](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/code/cves/2023/CVE-2023-49105.yaml) | | | | | | | |
| [CVE-2026-60004](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-60004) | Gitea / Gitea | 2026-08-25 | critical | 9.8 | 0.23988 (97.9%) | Unknown | Yes | CWE-94 |
| ↳ 检测模板 | [CVE-2026-60004](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-60004.yaml) | | | | | | | |
| [CVE-2026-73570](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-73570) | Synacor / Zimbra Collaboration Suite (ZCS) | 2026-08-21 | high | 8.9 | 0.76619 (99.6%) | Unknown | Yes | CWE-78 |
| ↳ 检测模板 | [CVE-2026-73570](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/network/cves/2026/CVE-2026-73570.yaml) | | | | | | | |
| [CVE-2026-64849](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-64849) | MLflow / MLflow | 2026-08-19 | critical | 9.3 | 0.10929 (96.0%) | Unknown | No | CWE-918 |
| ↳ 检测模板 | [CVE-2026-64849](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-64849.yaml) | | | | | | | |
| [CVE-2026-55040](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-55040) | Microsoft / SharePoint | 2026-08-18 | critical | 9.1 | 0.69536 (99.4%) | Unknown | Yes | CWE-1390 |
| ↳ 检测模板 | [CVE-2026-55040](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/javascript/cves/2026/CVE-2026-55040.yaml) | | | | | | | |
| [CVE-2025-62593](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2025-62593) | Ray-Project / Ray | 2026-08-17 | critical | 9.8 | 0.60150 (99.2%) | Unknown | Yes | CWE-94, CWE-352 |
| ↳ 检测模板 | [CVE-2025-62593](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2025/CVE-2025-62593.yaml) | | | | | | | |
| [CVE-2026-72898](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-72898) | Metabase / Metabase | 2026-08-11 | critical | 10.0 | 0.98513 (99.9%) | Unknown | Yes | CWE-89 |
| ↳ 检测模板 | [CVE-2026-72898](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-72898.yaml) | | | | | | | |
| [CVE-2026-63077](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-63077) | JetBrains / TeamCity | 2026-08-05 | critical | 9.8 | 0.93936 (99.8%) | Known | Yes | CWE-502 |
| ↳ 检测模板 | [CVE-2026-63077](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-63077.yaml) | | | | | | | |
| [CVE-2026-9198](https://www.cisa.gov/known-exploited-vulnerabilities-catalog?field_cve=CVE-2026-9198) | IBM / Langflow | 2026-08-04 | critical | 9.8 | 0.28658 (98.2%) | Unknown | Yes | CWE-94 |
| ↳ 检测模板 | [CVE-2026-9198](https://github.com/projectdiscovery/nuclei-templates/blob/a346d8f00c61bf95bbd30365f12cf923be51ba3d/http/cves/2026/CVE-2026-9198.yaml) | | | | | | | |

## 使用说明

- 优先查看 CISA `requiredAction` 与厂商修复公告；KEV 收录是修复优先级信号。
- Nuclei CVSS/EPSS 是模板自带分类字段，可能滞后；后续应独立接入 NVD 与 FIRST EPSS 并保留来源和更新时间。
- Nuclei 模板只用于已授权的检测范围；执行前仍需单独确认目标授权、版本、参数和速率限制。
- CISA CVE 条目的 `cvss_score` / `epss_score` 仍保持 null；不会把模板分数冒充 CISA 字段。
