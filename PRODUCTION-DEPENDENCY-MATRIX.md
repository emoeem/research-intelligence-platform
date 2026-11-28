# RIP Production Dependency Matrix

> Status: P0 inventory

| Layer | Main implementation | Runtime/dependency | Current deployment expectation | P0/P1 action |
|---|---|---|---|---|
| Rust workspace | 25 crates in 25 directories; 25 Cargo manifests | rust-version `1.99`, vendored/offline build | Host cargo; native-first per repository policy | Recheck toolchain and offline vendor in clean environment |
| SQLite | rusqlite 0.41.1 bundled, 33 migrations | No external SQLite service | Local platform DB and dev/test DBs | Define which DB is production, migration owner and backup schedule |
| PostgreSQL | PG 17 + refinery + pgvector | `RP_PLATFORM_PG_DSN`, PG test containers | Isolated PG for some integration tests | Formal DSN, credentials, network, backup and restore policy |
| API | Python stdlib `apps/server/rp_server.py` | Python 3.10+; Bearer auth; systemd user service | 7800 dev/old entry; 7815 temporary evidence | Sign production entry/TLS and service manager |
| Web | React 19/Vite 8/TypeScript 7 | Node >=22.12 | Optional frontend build/dev/proxy | Verify deployed assets, API routes and real persistence |
| Python workers | stdlib/locked worker dependencies | Project/system runtime or container fallback | Embedding container pinned and present; parser absent | No uv/system pip; host-first, container for isolated conflicts |
| Parser | Document Router/Rust + container parser worker | Exact `rip-parser` image digest absent | Podman fallback only | Recover image/digest and run Golden Corpus |
| Embedding | ONNX Runtime worker | Pinned `rip-embed` digest present | Podman/network-none | Keep 512 dims/model/backend verification; no silent fallback |
| Search | Unified search, SQLite FTS/PG hybrid | pgvector, search projection/index DB | Local/auxiliary DB projections | Define index rebuild/refresh and human metrics |
| MCP | official rmcp Rust SDK 3.10.0 | stdio transport | Native Rust server | Keep protocol parity and caller workspace restrictions |
| Tool Engine | native/Python/Podman adapters | Podman 6.2.1 observed | Native-first; Podman isolation when needed | Verify manifest, digest, network/file permissions and timeouts |
| Workflow | Rust workflow engine | SQLite/PG persistence | Local service state | Verify checkpoint, cancel propagation and restart recovery |
| Containers | 56 Containerfile/manifests | Podman | Limited tool/runtime images | Pin images/digests and remove test containers after proof |

## Python and JavaScript dependency evidence

- Python lock: `workers/python/requirements.lock.txt` includes hashes.
- Worker lock must remain in project tooling; no global pip or system Python
  overwrite is allowed.
- Web lock/package state is in `web/package.json` and its package lock.
- Vendor: Rust vendor data is present and the repository requires offline build
  verification.
