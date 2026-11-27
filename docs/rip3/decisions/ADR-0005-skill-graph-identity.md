# ADR-0005 — Skill identities in the unified knowledge graph

- Status: Accepted for schema and candidate-pipeline implementation; production batch review remains pending.
- Date: 2026-11-27

## Context

The Platform SQLite `skill` registry contains 125 Skill records, but `required_tools` and `knowledge_refs` are empty. The M1 PostgreSQL graph already owns cross-authority identities, immutable source snapshots, evidence links, relation candidates, review logs and source trust/license assessments. Creating a parallel Skill graph or copying Skill bodies into Knowledge Cards would split authority.

## Decision

1. Add `skill` to `KnowledgeEntityType` and `knowledge_entity_refs` via V0013. The Skill registry remains authoritative for Skill metadata and body; this graph stores identity and source-version references only.
2. Represent Skill→Tool associations as `USES_TOOL` from `skill` to `security_tool`. Represent Skill references to vulnerabilities, Nuclei detection templates, POCs, Knowledge Cards and technical documents as `REFERENCES`. Relations are always `CANDIDATE` on insertion.
3. Attach the immutable Skill-body snapshot as an Evidence entity and `knowledge_relation_evidence` link. A relation cannot be approved until at least one current, hashed, URI-addressable Evidence source has an explicitly approved trust/license assessment.
4. Tool candidates use exact lexical evidence from Skill bodies. CVE/Template candidates require an exact CVE identifier in the Skill body and an exact match against the active KSE Nuclei `classification.cve-id`. Do not infer requiredness from mere mentions, fuzzy titles or model confidence. Negative tool mentions are excluded. Heuristic scores are evidence metadata, not calibrated probabilities.
5. `rp-skill-map` and `rp-skill-ref-map` are dry-run by default. `--apply` creates/reuses typed Skill, target and Evidence identities and candidate relations atomically. They do not update SQLite projections or approve anything.
6. Only after batch review may approved Tool relations be projected into `skill.required_tools`. Skill→Knowledge/Template relations remain graph references until their review is complete; no automatic `knowledge_refs` backfill is authorized by this ADR.

## Consequences

- Existing graph infrastructure is reused; no second graph or facts database is introduced.
- Skill body changes must advance the Skill source version; an immutable source snapshot conflict is fail-closed.
- Tool health and permissions remain independent runtime gates. A reviewed mapping does not grant execution permission.
- Candidate-generation output is reproducible and auditable by body hash, line number, snippet and source registry row.

## Acceptance

- Domain type round-trip includes `skill`.
- V0013 applies and is idempotent on an isolated PostgreSQL instance.
- Atomic Skill→Tool candidate creation registers typed identities, attaches Evidence, remains `CANDIDATE`, and refuses approval while the Evidence assessment is `UNREVIEWED`.
- Candidate generation is read-only and its tests verify path containment, exact evidence, negative-mention exclusion and exact CVE-to-KSE-template joins.
- Disposable PostgreSQL acceptance imports the complete 146 Skill→Tool batch and 173 Skill→Vulnerability/Template batch, then replays both with zero duplicate relations/evidence links.
- Dry-run is the default; explicit apply is tested only against disposable PostgreSQL containers.
