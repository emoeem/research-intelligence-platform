# MCP Knowledge Candidate Envelope

## Status

Normative candidate-write contract for external AI clients through MCP.

The following MCP tools are available:

- `rip.search_knowledge`
- `rip.get_knowledge_detail`
- `rip.list_knowledge_review_queue`
- `rip.submit_knowledge_candidate`
- `rip.review_knowledge_candidate`
- `rip.get_knowledge_ingestion_status`
- `rip.get_knowledge_ingestion_candidate`

`rip.review_knowledge_candidate` is intentionally limited to:

- `quarantine`
- `reject`
- `candidate`
- `deprecate`

MCP AI callers are not allowed to use `approve`. Approval remains an explicit
human/service review operation under the existing `rp-kc review` and Card
review-state gate.

## Envelope

```json
{
  "contract_version": "1.0",
  "idempotency_key": "provider-run-source-stable-id",
  "knowledge_type": "vuln_signature",
  "title": "Knowledge title",
  "summary": "Short summary",
  "content": "Structured or prose knowledge content",
  "confidence": 0.75,
  "source": {
    "type": "document",
    "title": "Source title",
    "uri": "https://example.invalid/source",
    "content_sha256": "64lowercasehex",
    "license": "declared-license-or-unknown",
    "source_commit": "optional commit or version",
    "accessed_at": "2026-11-28T00:00:00Z",
    "parser_name": "poppler",
    "parser_version": "26.11.0"
  },
  "evidence": [
    {
      "artifact_id": "art-...",
      "tool_run_id": "rn-...",
      "trace_id": "trace-...",
      "document_id": "doc-...",
      "revision_id": "drev-...",
      "block_id": "blk-...",
      "page": 1,
      "char_start": 100,
      "char_end": 260,
      "quote_sha256": "64lowercasehex"
    }
  ],
  "risk_flags": []
}
```

## Required invariants

1. `contract_version` must be exactly `1.0`.
2. `idempotency_key` must be 8-256 bytes and stable for the source operation.
3. `knowledge_type` must use the existing domain vocabulary:
   `vuln_principle`, `vuln_signature`, `exploit_condition`,
   `detection_method`, `fix_method`, `security_technique`, `tool_usage`,
   or `research_case`.
4. `title` and `content` must be non-empty.
5. `source.content_sha256` must be 64 lowercase hexadecimal characters.
6. At least one evidence item must be supplied.
7. Evidence positions must belong to the supplied artifact/document evidence
   and must not be fabricated by the AI client.
8. `confidence` must be in `[0,1]`.
9. Risk flags use existing RIP risk vocabulary.
10. `workspace` is caller-bound and fixed to `default` until caller-workspace
    ACL exists. MCP callers cannot override it.

## State transitions

Candidate submission creates a knowledge card in `CANDIDATE` state. It is
allowed to create review and duplicate candidates, but it must not:

- set `APPROVED`;
- publish a fact;
- merge or delete an existing card;
- bypass evidence requirements;
- bypass source hash and license constraints;
- reuse an idempotency key for a different payload.

The existing `rp-kc` review gates remain authoritative. Approval requires
quality scoring, no hard risk flags and an explicit review transition.

## Duplicate policy

Classification is not an AI-only merge command:

- exact normalized hash: `SAME_CONTENT` candidate or existing card reference;
- high semantic similarity: `NEAR_DUPLICATE` review suggestion;
- related but independently useful: `RELATED_TO`;
- independent content: create a distinct candidate.

Only a merge/reuse decision may link an existing card. MCP never deletes or
silently merges content.

## Evidence policy

Candidate evidence may include:

- document/revision/block IDs;
- page and character offsets;
- artifact/tool-run/trace IDs;
- exact quote hashes;
- source URI, commit or license.

The result is untrusted until independently verified. Evidence is provenance,
not approval.

## MCP failure categories

- `VALIDATION`: envelope, workspace, hash, evidence or state transition invalid.
- `POLICY_DENIED`: caller unknown or revoked.
- `INFRA`: `rp-kc`/PostgreSQL unavailable or subprocess failure.
- `EXECUTION`: MCP subprocess ended without a usable result.

## Publication replay (PB-010 / PB-012 acceptance)

The full candidate-to-publication chain is regression-locked by two
executable replay surfaces (2026-11-28):

1. Library level: `crates/intel-db/tests/publication_replay.rs`
   (`RP_TEST_PG_DSN` gated, 2 tests): donor submit lands `CANDIDATE` only;
   default search never returns a candidate while
   `include-unapproved` proves the bm25 index holds the row and the
   `review_status` predicate is the gate; approve before scoring is blocked
   ("quality score not computed yet"); the approval gate matrix blocks hard
   risk flags and `REJECTED -> APPROVED` transitions; after human approval the
   default search returns the card with no separate index rebuild step.
2. Binary level: `apps/server/selftest_knowledge_publish.sh` (18/18, isolated
   `rip-pg17` container): real `rp-mcp` over stdio JSON-RPC with a caller
   resolved through `rp-admin caller-add`/`key-gen`; MCP submit receipt is
   `CANDIDATE`; MCP `approve` is rejected ("MCP never approves"); the
   human/service approval runs through `rp-kc score` + `rp-kc review
   --decision approve`; the post-approval MCP search returns the card; the
   review log carries both the donor and the human rows; production
   `migrations/platform*.db` fingerprints are unchanged.

Concurrent first-migration note: rmcp may dispatch parallel `tools/call`
requests, so two `rp-kc` processes can run refinery migrations on a fresh
database at the same time; `intel_db::run_migrations` serializes this with a
`pg_advisory_lock` (session-scoped, released before the pool returns the
connection).
