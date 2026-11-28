# RIP Competitor Benchmark and Optimization Roadmap

> Date: 2026-11-28
> Scope: current RIP repository plus OpenCTI, IntelOwl, SpiderFoot and Nuclei
> Recommendation: production closure before feature expansion

## 1. Executive conclusion

RIP is broader than a simple scanner or OSINT collector: it combines parsing,
knowledge governance, search, MCP tools, Agent and Workflow execution, and a
React workbench. That is strategically useful, but it is not yet a mature
operating system for threat intelligence.

The highest-value next work is not adding a larger model or more UI pages.
It is to finish:

1. a host-first parser and evidence pipeline;
2. standardized entity and connector interoperability;
3. human-labeled search and deduplication quality;
4. a complete Raw/Candidate/Published lifecycle;
5. formal PostgreSQL backup and recovery;
6. real AI Provider and workflow E2E;
7. production deployment, TLS and observability.

## 2. Comparator baseline

| Project | Official positioning | Strong production capability | What RIP should reuse or learn |
|---|---|---|---|
| OpenCTI | Open cyber threat intelligence platform | STIX-oriented graph, entities/relationships, reports, cases, incidents, observations, connectors, Elasticsearch/Redis/RabbitMQ/S3 scale-out | Adopt a standard intelligence object/relationship model and connector interoperability; do not reimplement its graph engine |
| IntelOwl | Threat intelligence at scale | Analyzer plugins, observables/files, playbooks, pivots, investigations, REST APIs, GUI, exports/connectors | Standardize analyzer/result schemas, playbooks and investigation artifacts; reuse an existing maturity pattern |
| SpiderFoot | OSINT automation and attack-surface collection | Over 200 modules, publisher/subscriber data flow, correlation rules, SQLite, CLI/UI, visualisation, TOR and third-party tool integration | Add target-to-observable provenance, module scheduling, correlation rules and change detection |
| Nuclei | YAML-template vulnerability detection | Fast parallel scanning, reusable templates, protocol coverage, rate/scan controls, CI/CD and finding integrations | Reuse Nuclei as the mature execution engine; do not replace its template engine or scanner |
| RIP current | Integrated research intelligence workbench | Rust/PG/SQLite, MCP, tools, workflows, Agent, evidence, audit, parser and knowledge schemas | Strong broad architecture; weaknesses are in closing real data and operational loops |

Official sources:

- [OpenCTI architecture and deployment](https://docs.opencti.io/latest/deployment/overview/)
- [OpenCTI data and UI model](https://docs.opencti.io/latest/usage/overview/)
- [OpenCTI GitHub](https://github.com/OpenCTI-Platform/opencti)
- [IntelOwl README](https://github.com/intelowlproject/IntelOwl)
- [SpiderFoot README](https://github.com/smicallef/spiderfoot)
- [Nuclei README](https://github.com/projectdiscovery/nuclei)

## 3. RIP gaps ranked by production impact

### P0: parser and ingest execution

The exact `rip-parser` container digest is optional, not mandatory, because
the host already has mature parsers:

- Poppler for PDF text and metadata
- Tesseract for OCR
- LibreOffice for DOCX/PPTX/XLSX
- Pandoc for Markdown/HTML/text conversion
- FFmpeg/ExifTool/MediaInfo for media metadata
- qpdf for PDF structure and archive checks

Gap: these capabilities are not yet a verified end-to-end Golden Corpus with
host engine identity, format routing, document IR, evidence offsets and
idempotent replays.

Recommendation:

- implement host adapters that call the existing tools through existing
  `Tool Engine`/MCP contracts;
- use PyMuPDF or Apache Tika only if a head-to-head host format test proves
  a concrete gap;
- keep Docling as an optional isolated fallback for complex layout;
- do not add a second parser dispatcher or hand-written PDF/Office parser.

### P0: production data protection

SQLite backup/restore mechanism evidence passes, but:

- PostgreSQL policy and named recovery owner are not signed;
- SQLite/PG cross-database restore timing is not formally accepted;
- formal offsite/encrypted backup and RTO/RPO are not defined.

Recommendation:

- reuse `rp-admin backup/restore`, PostgreSQL `pg_dump/pg_restore` and existing
  migration health code;
- publish a single backup owner and schedule;
- execute independent PostgreSQL and file/artifact restoration.

### P0: knowledge publication

The Candidate/review model exists, but it has not closed the complete factual
publication and index-refresh loop.

Recommendation:

- require evidence and source assessment for every factual candidate;
- preserve all publish/reject/reuse/undo audit records;
- refresh or atomically rebuild search indexes after publication;
- make retries idempotent and observable.

### P0: real AI closure

Mock-provider and keyword-planner evidence is useful but not production
acceptance.

Recommendation:

- inject one DeepSeek-compatible provider through existing
  `RIP_LLM_*` environment contracts;
- run one real Provider research chain: search, evidence, tool call, candidate,
  human review, publish and re-search;
- never log prompt bodies or credentials;
- keep tool execution behind existing capability/workspace/caller/budget gates.

### P1: standardized CTI interoperability

RIP has strong internal entities and evidence, but OpenCTI demonstrates why
STIX 2.1 and external connector/export compatibility are important.

Recommendation:

- evaluate `stix2` for Python/STIX serialization and official Taxii libraries
  for TAXII 2.1 import/export;
- add STIX bundle import/export as adapter/projection, not as an internal
  second knowledge model;
- support MISP and OpenCTI export only after mapping and licensing review.

### P1: search quality

The code has keyword, vector and hybrid search, but human relevance sets are
missing.

Recommendation:

- freeze independent human-labeled retrieval and deduplication splits;
- report Recall@K, MRR and NDCG;
- report Duplicate/Related/Distinct precision, recall and false-merge rate;
- expose abstention and no-answer state in the API/UI.

### P1: OSINT and enrichment modules

SpiderFoot shows the value of a mature publisher/subscriber module ecosystem,
while RIP currently has only a comparatively small registry.

Recommendation:

- build a target/observable normalization layer;
- use existing modules/tools rather than implementing DNS/WHOIS/BGP parsers;
- add schedulers, source ETag/change detection and correlation rules;
- keep network, credentials and target scope in Tool Manifests.

### P1: operations

There is an API metrics endpoint and user systemd support, but no formal SLO,
alerting, trace-dashboard, TLS cutover or disaster-recovery sign-off.

Recommendation:

- define health, queue age, PG/SQLite latency, parser failure, LLM error,
  workflow failure, disk and certificate alerts;
- deploy a reverse proxy/TLS only after a formal cutover and rollback drill;
- keep Prometheus/OpenTelemetry as adapters; do not add a second audit system.

### P2: architecture simplification

The project has 25 Rust crates and a large migration surface. It is broad, but
some overlapping concepts should be consolidated:

- SQLite and PostgreSQL duties should be explicit;
- platform database copies should be distinguished from test snapshots;
- MCP, REST and CLI should retain one shared dispatch/registry fact source;
- workflow, Agent and Tool results should share trace/evidence schemas.

## 4. Mature-library priority plan

| Capability | Preferred mature solution | Host-first execution | Container fallback |
|---|---|---|---|
| PDF text/layout | Poppler/MuPDF/PyMuPDF where tested | Yes | Complex PDF/Docling |
| OCR | Tesseract, RapidOCR | Yes | RapidOCR/ONNX isolation |
| Office | LibreOffice, python-docx/pptx/openpyxl as format-specific adapters | Yes | Heavy layout or dependency isolation |
| HTML/Markdown | Pandoc, trafilatura/readability-type library after evaluation | Yes | None normally |
| CTI exchange | official STIX2 and TAXII clients | Native library/worker | Not required |
| Enrichment modules | existing SpiderFoot/IntelOwl patterns or mature SDKs | Host tool/worker | Only isolation or dependency conflict |
| Vulnerability scanning | existing Nuclei | Yes for CLI/API | Podman for policy isolation |
| Search | existing PG FTS, pgvector, PostgreSQL | Yes | Not required |
| Workflow | existing `workflow-engine` | Yes | Tool nodes may use Podman |
| MCP | existing official `rmcp` | Yes | External MCP isolation |
| Observability | existing metrics + exporter/OTel integration | Yes | Not required |

## 5. Host package gaps

Already available:

- `tesseract`
- `libreoffice-fresh` or the host equivalent
- `pandoc`
- `qpdf`
- `p7zip`
- `exiftool`
- `ffmpeg`/`ffprobe`
- `mediainfo`
- `chromium`
- `unzip`

Missing or not currently confirmed:

| Package | Purpose | Priority |
|---|---|---|
| `poppler` / `poppler-utils` | canonical PDF text and page tools | P0 |
| `mupdf` | `mutool` alternative fallback for PDFs/images | P1 |
| `antiword` | legacy `.doc` text extraction | P1 |
| `unrtf` | legacy RTF text extraction | P1 |
| `python-jsonschema` | host JSON Schema validation | P1 |
| `python-magic` / libmagic | file-type magic library | P1 |
| `clamav` | real file malware scan | P1/P2 |
| `yara` | static rule scanning | P1/P2 |
| `ocrmypdf` | digital/scan PDF normalization | P1 only after format gap test |

No package should be installed globally with `pip` or used to overwrite
Pacman-managed Python modules. Python project dependencies should be locked
in project tooling or isolated only when host dependency conflicts require it.

## 6. Recommended delivery sequence

### Batch 1: data correctness (1-2 weeks)

1. Host parser adapter and format routing.
2. 40+ real, hashed, licensed/attributed Golden Corpus.
3. Document IR/evidence-position acceptance.
4. Idempotent repeat and failure-recovery tests.

### Batch 2: knowledge quality (1-2 weeks)

1. Human Duplicate/Related/Distinct labels.
2. Human retrieval qrels and independent splits.
3. Recall@K/MRR/NDCG plus semantic metrics.
4. Candidate publication/index-refresh closure.

### Batch 3: production operations (1-2 weeks)

1. PostgreSQL backup/restore policy.
2. Independent recovery evidence.
3. Migration replay/upgrade/rollback gate.
4. TLS, reverse proxy and formal cutover.

### Batch 4: real AI and OSINT closure

1. DeepSeek real-provider E2E.
2. Real MCP/Tool/Agent/Workflow evidence chain.
3. Target/observable normalization and source scheduling.
4. STIX/TAXII and selected MISP/OpenCTI adapters.

## 7. Exit gates

RIP should not be called production-ready until:

- all promised formats pass the real corpus;
- factual Candidate/Published lifecycle and re-index refresh pass;
- independent human retrieval metrics are calibrated;
- PostgreSQL and file recovery pass in an independent environment;
- real Provider E2E cites evidence and survives failure;
- workflow interruption and service restart pass;
- the production entry passes TLS, authorization and rollback;
- no unresolved P0/P1 database, security or parser blocker remains.
