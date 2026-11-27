#!/usr/bin/env python3
"""Validate RTIS curation overlay without modifying the source knowledge DB."""
from __future__ import annotations
import json
import sqlite3
import subprocess
import sys
from urllib.parse import urlparse
from pathlib import Path
from poc_assessment import (
    validate_assessment_defaults,
    validate_local_artifact_hash,
    validate_poc_assessment,
)

ROOT = Path(__file__).resolve().parents[3]
WORKSPACE_ROOT = ROOT.parent
OVERLAY = Path(__file__).with_name('k_meta_overlay.json')
DB = ROOT / 'integrations/rtis/knowledge.db'
LEVELS = {'K0', 'K1', 'K2', 'K3', 'K4'}
LIFECYCLES = {'Discovered', 'Candidate', 'Researched', 'Lab-validated', 'Field-validated', 'Verified', 'Deprecated', 'Rejected'}
RELATIONS = {'supports', 'contradicts', 'requires', 'supersedes', 'derived_from', 'uses_tool', 'has_case', 'recommends'}


def fail(message: str) -> None:
    print(f'FAIL: {message}', file=sys.stderr)
    raise SystemExit(1)


data = json.loads(OVERLAY.read_text(encoding='utf-8'))
if data.get('schema_version') != 1:
    fail('schema_version must be 1')
try:
    validate_assessment_defaults(data.get('poc_assessment_defaults'))
except ValueError as exc:
    fail(str(exc))
entries = data.get('entries')
relations = data.get('relations')
external_entities = data.get('external_entities', [])
if not isinstance(entries, list) or not isinstance(relations, list) or not isinstance(external_entities, list):
    fail('entries, external_entities and relations must be arrays')

conn = sqlite3.connect(f'file:{DB}?mode=ro', uri=True)
seen: set[int] = set()
for e in entries:
    kid = e.get('knowledge_id')
    if not isinstance(kid, int) or kid in seen:
        fail(f'invalid or duplicate knowledge_id: {kid!r}')
    seen.add(kid)
    row = conn.execute('SELECT code FROM poc WHERE id=?', (kid,)).fetchone()
    if not row or row[0] != e.get('code'):
        fail(f'POC id/code mismatch for {kid}: expected {e.get("code")!r}, got {row!r}')
    if conn.execute('SELECT 1 FROM k_meta WHERE knowledge_id=?', (kid,)).fetchone():
        fail(f'RTIS already has authoritative k_meta for {kid}; remove overlay row')
    level = e.get('evidence_level')
    lifecycle = e.get('lifecycle')
    if level not in LEVELS or lifecycle not in LIFECYCLES:
        fail(f'invalid level/lifecycle for {kid}: {level!r}/{lifecycle!r}')
    sources = e.get('source_urls') or []
    refs = e.get('evidence_refs') or []
    if level == 'K1' and not sources:
        fail(f'K1 requires source_urls: {kid}')
    if level == 'K2' and not refs:
        fail(f'K2 requires evidence_refs: {kid}')
    if level == 'K3' and not (refs and e.get('experiment_ref') and e.get('rerun_ref')):
        fail(f'K3 requires evidence_refs, experiment_ref and rerun_ref: {kid}')
    if level == 'K4' and len(set(e.get('environment_refs') or [])) < 2:
        fail(f'K4 requires at least two distinct environment_refs: {kid}')
    if lifecycle == 'Researched' and not (sources or refs):
        fail(f'Researched requires a source document or URL: {kid}')
    if lifecycle == 'Lab-validated' and not (e.get('experiment_ref') and e.get('rerun_ref')):
        fail(f'Lab-validated requires reproducible experiment references: {kid}')
    if lifecycle == 'Field-validated' and not e.get('outcome_ref'):
        fail(f'Field-validated requires a structured outcome_ref: {kid}')
    if lifecycle == 'Verified' and not (e.get('human_review_ref') and len(set(e.get('environment_refs') or [])) >= 2):
        fail(f'Verified requires human_review_ref and two distinct environments: {kid}')
    if e.get('redact_verified_note') is True and not (isinstance(e.get('redacted_verified_note'), str) and e['redacted_verified_note'].strip()):
        fail(f'redact_verified_note requires a non-empty redacted_verified_note: {kid}')
    source_urls = e.get('source_urls', [])
    if not isinstance(source_urls, list):
        fail(f'source_urls must be an array: {kid}')
    for url in source_urls:
        parsed = urlparse(url) if isinstance(url, str) else None
        if not parsed or parsed.scheme not in ('http', 'https') or not parsed.netloc:
            fail(f'invalid upstream source URL: {kid}: {url!r}')
    if len(source_urls) != len(set(source_urls)):
        fail(f'duplicate upstream source URLs: {kid}')
    source_path = e.get('source_path')
    if source_path:
        candidate = (ROOT / source_path).resolve()
        if ROOT.resolve() not in candidate.parents and candidate != ROOT.resolve():
            fail(f'source_path escapes repository: {kid}: {source_path}')
        if not candidate.exists():
            fail(f'source_path does not exist: {kid}: {source_path}')
    source_commit = e.get('source_commit')
    if source_commit:
        checked = subprocess.run(['git', 'cat-file', '-e', f'{source_commit}^{{commit}}'], cwd=ROOT,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if checked.returncode != 0:
            fail(f'source_commit is not present in repository history: {kid}: {source_commit}')
    if e.get('source_integrity') not in (None, 'complete', 'partial', 'missing'):
        fail(f'invalid source_integrity for {kid}: {e.get("source_integrity")!r}')
    if e.get('source_integrity') == 'missing' and not (e.get('missing_evidence') or []):
        fail(f'missing source_integrity requires explicit missing_evidence: {kid}')
    if e.get('verification_result') not in (None, 'candidate_not_verified', 'historical_evidence_supported', 'not_applicable'):
        fail(f'invalid verification_result for {kid}: {e.get("verification_result")!r}')
    if e.get('dedup_status') not in (None, 'independent', 'related-family', 'duplicate_candidate', 'duplicate'):
        fail(f'invalid dedup_status for {kid}: {e.get("dedup_status")!r}')
    if e.get('independent_knowledge_status') not in (None, 'independent', 'family-member', 'duplicate'):
        fail(f'invalid independent_knowledge_status for {kid}: {e.get("independent_knowledge_status")!r}')
    if e.get('missing_evidence') and not e.get('review_gate'):
        fail(f'missing_evidence requires an actionable review_gate: {kid}')
    assessment = e.get('poc_assessment')
    if assessment is not None:
        try:
            validate_poc_assessment(
                assessment,
                has_missing_evidence=bool(e.get('missing_evidence')),
            )
        except ValueError as exc:
            fail(f'POC assessment for {kid}: {exc}')
        for ref in assessment.get('evidence_refs', []):
            if ref.startswith(('http://', 'https://')):
                parsed = urlparse(ref)
                if parsed.scheme != 'https' or not parsed.netloc:
                    fail(f'assessment evidence URL must be HTTPS: {kid}: {ref}')
                continue
            candidate = (ROOT / ref.split('#', 1)[0]).resolve()
            if WORKSPACE_ROOT.resolve() not in candidate.parents or not candidate.is_file():
                fail(f'assessment evidence ref does not resolve inside workspace: {kid}: {ref}')
        for artifact in assessment.get('evidence_artifacts', []):
            ref = artifact['ref']
            if ref.startswith(('http://', 'https://')):
                parsed = urlparse(ref)
                if parsed.scheme != 'https' or not parsed.netloc:
                    fail(f'evidence artifact URL must be HTTPS: {kid}: {ref}')
                continue
            candidate = (ROOT / ref.split('#', 1)[0]).resolve()
            if WORKSPACE_ROOT.resolve() not in candidate.parents or not candidate.is_file():
                fail(f'evidence artifact does not resolve inside workspace: {kid}: {ref}')
            try:
                validate_local_artifact_hash(candidate, artifact['sha256'])
            except ValueError as exc:
                fail(f'POC assessment for {kid}: {exc}')
    for ref in refs:
        local = ref.split('#', 1)[0]
        if local.startswith(('http://', 'https://')):
            continue
        candidate = (ROOT / local).resolve()
        if WORKSPACE_ROOT.resolve() not in candidate.parents and candidate != WORKSPACE_ROOT.resolve():
            fail(f'evidence ref escapes workspace: {kid}: {ref}')
        if not candidate.is_file():
            fail(f'evidence ref does not exist: {kid}: {ref}')
    for url in sources:
        if not isinstance(url, str) or not url.startswith('https://'):
            fail(f'source URL must be HTTPS: {kid}: {url!r}')

entry_codes = {e['code'] for e in entries}
external_codes: set[str] = set()
for entity in external_entities:
    code = entity.get('code')
    if not isinstance(code, str) or not code.strip() or code in entry_codes or code in external_codes:
        fail(f'invalid or duplicate external entity code: {code!r}')
    if not entity.get('entity_type') or not entity.get('title'):
        fail(f'external entity requires entity_type and title: {code}')
    external_codes.add(code)

for edge in relations:
    if edge.get('type') not in RELATIONS:
        fail(f'unsupported relation type: {edge!r}')
    if not (0 <= float(edge.get('confidence', -1)) <= 1):
        fail(f'invalid relation confidence: {edge!r}')
    if not edge.get('src') or not edge.get('dst') or edge['src'] == edge['dst']:
        fail(f'invalid relation endpoints: {edge!r}')
    if edge['src'] not in entry_codes | external_codes:
        fail(f'unresolved relation source: {edge["src"]}')
    if edge['dst'] not in entry_codes | external_codes:
        fail(f'unresolved relation target: {edge["dst"]}')

conn.close()
print(f'PASS: {len(entries)} curation entries; {len(external_entities)} external entities; {len(relations)} resolved relations; source DB read-only')
print('Evidence levels:', {k: sum(e['evidence_level'] == k for e in entries) for k in sorted(LEVELS)})
print('Lifecycles:', {k: sum(e['lifecycle'] == k for e in entries) for k in sorted({e['lifecycle'] for e in entries})})
