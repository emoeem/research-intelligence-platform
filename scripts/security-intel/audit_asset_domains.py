#!/usr/bin/env python3
"""Read-only, domain-separated inventory and provenance audit for RIP assets."""
from __future__ import annotations
import argparse, json, sqlite3
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULTS = {
    "platform": ROOT / "migrations/platform.db",
    "plugin": ROOT / "migrations/dev-p6.db",
    "rtis": ROOT / "integrations/rtis/knowledge.db",
    "search": ROOT / "rip-data/derived/curated-poc-search.db",
}

def db(path: Path) -> sqlite3.Connection:
    if not path.is_file(): raise FileNotFoundError(path)
    c = sqlite3.connect(f"file:{path.resolve()}?mode=ro", uri=True)
    c.row_factory = sqlite3.Row
    c.execute("PRAGMA query_only=ON")
    return c

def rows(c: sqlite3.Connection, sql: str) -> list[dict[str, Any]]:
    return [dict(r) for r in c.execute(sql)]

def one(c: sqlite3.Connection, sql: str) -> dict[str, Any]:
    r=c.execute(sql).fetchone(); return dict(r) if r else {}

def audit(platform: Path, plugin: Path, rtis: Path, search: Path) -> dict[str, Any]:
    out: dict[str, Any] = {"as_of":"2026-11-27", "mode":"read-only", "scope":{}, "domains":{}}
    with db(platform) as p, db(plugin) as k, db(rtis) as r, db(search) as s:
        out["integrity"]={"platform":p.execute('PRAGMA integrity_check').fetchone()[0],"plugin":k.execute('PRAGMA integrity_check').fetchone()[0],"rtis":r.execute('PRAGMA integrity_check').fetchone()[0],"search":s.execute('PRAGMA integrity_check').fetchone()[0]}
        active=[x[0] for x in k.execute('SELECT plugin_id FROM plugin WHERE is_active=1')]
        active_sql='(plugin_id,plugin_version) IN (SELECT plugin_id,version FROM plugin WHERE is_active=1)'
        out["scope"]={"active_plugins":active,"historical_plugin_rows":int(k.execute('SELECT COUNT(*) FROM knowledge_source_entry').fetchone()[0]),"active_source_rows":int(k.execute(f'SELECT COUNT(*) FROM knowledge_source_entry WHERE {active_sql}').fetchone()[0]),"search_projection_rows":int(s.execute('SELECT COUNT(*) FROM search_index_doc').fetchone()[0])}
        # Knowledge and general research resources
        out['domains']['knowledge']={
          'platform_cards':one(p,"SELECT COUNT(*) total, SUM(lifecycle='VERIFIED') verified, SUM(lifecycle='CANDIDATE') candidate, SUM(lifecycle='DISCOVERED') discovered FROM knowledge"),
          'cards_by_kind_lifecycle':rows(p,"SELECT record_kind,lifecycle,COUNT(*) n FROM knowledge GROUP BY record_kind,lifecycle ORDER BY record_kind,lifecycle"),
          'resource_catalog':rows(p,"SELECT quality_grade,COUNT(*) n FROM resource GROUP BY quality_grade ORDER BY quality_grade"),
          'resource_provenance':one(p,"SELECT COUNT(*) total,SUM(canonical_url<>'') canonical_url,SUM(source_status<>'') source_status FROM resource"),
          'resource_source_links':one(p,"SELECT COUNT(*) links,COUNT(DISTINCT resource_id) resources,SUM(source_url<>'') source_url,SUM(source_commit<>'') source_commit FROM resource_source"),
          'active_sources':rows(k,f"SELECT e.plugin_id,p.version,p.license,p.source_type,p.source_ref,COUNT(*) total,SUM(e.review_status='APPROVED') approved,SUM(e.review_status='DISCOVERED') discovered,SUM(e.review_status='REJECTED') rejected,SUM(length(e.body_bytes)>0) body,SUM(length(e.description)>0) description,SUM(length(e.canonical_url)>0) canonical_url,SUM(length(e.source_url)>0) source_url,SUM(length(e.source_commit)>0) source_commit,SUM(length(e.license_status)>0) license_pinned,COUNT(DISTINCT e.item_key) unique_keys,COUNT(DISTINCT e.content_sha256) unique_content FROM knowledge_source_entry e JOIN plugin p ON p.plugin_id=e.plugin_id AND p.version=e.plugin_version WHERE p.is_active=1 AND e.plugin_id=p.plugin_id AND e.plugin_version=p.version GROUP BY e.plugin_id ORDER BY e.plugin_id"),
          'search_projection':rows(s,"SELECT resource_type,COUNT(*) n FROM search_index_doc GROUP BY resource_type ORDER BY n DESC")
        }
        # Vulnerabilities: authoritative source catalogs vs references in weapon DB are kept distinct.
        out['domains']['vulnerabilities']={
          'source_catalogs':rows(k,f"SELECT plugin_id,COUNT(*) records,COUNT(DISTINCT item_key) unique_keys,COUNT(DISTINCT content_sha256) unique_content FROM knowledge_source_entry WHERE plugin_id IN ('knowledge.cisa-kev','knowledge.pocindex-kev') AND {active_sql} GROUP BY plugin_id ORDER BY plugin_id"),
          'rtis_cve_coverage':one(r,"SELECT COUNT(*) weapon_rows,SUM(cve IS NOT NULL AND trim(cve)<>'') rows_with_cve,COUNT(DISTINCT CASE WHEN cve IS NOT NULL AND trim(cve)<>'' THEN upper(trim(cve)) END) distinct_cves,SUM(cve IS NOT NULL AND trim(cve)<>'' AND upper(trim(cve)) NOT LIKE 'CVE-%') malformed_cve FROM poc"),
          'rtis_cve_source':rows(r,"SELECT COALESCE(source,'(null)') source,COUNT(*) records,COUNT(DISTINCT CASE WHEN cve IS NOT NULL AND trim(cve)<>'' THEN upper(trim(cve)) END) distinct_cves FROM poc GROUP BY source ORDER BY records DESC"),
          'cve_duplicate_groups':one(r,"SELECT COUNT(*) groups FROM (SELECT upper(trim(cve)) FROM poc WHERE cve IS NOT NULL AND trim(cve)<>'' GROUP BY upper(trim(cve)) HAVING COUNT(*)>1)"),
          'severity_distribution':rows(r,"SELECT COALESCE(NULLIF(lower(trim(severity)),''),'(missing)') severity,COUNT(*) n FROM poc GROUP BY COALESCE(NULLIF(lower(trim(severity)),''),'(missing)') ORDER BY n DESC")
        }
        # POCs and non-Nuclei executable/reference material.
        out['domains']['pocs']={
          'non_nuclei_total':one(r,"SELECT COUNT(*) total,COUNT(DISTINCT code) distinct_codes,SUM(verified=1) legacy_verified_flag,SUM(source IS NULL OR trim(source)='') missing_source,SUM(source_url IS NULL OR trim(source_url)='') missing_source_url,SUM(description IS NULL OR trim(description)='') missing_description,SUM(content IS NULL OR trim(content)='') missing_content,SUM(versions IS NULL OR trim(versions)='') missing_versions FROM poc WHERE lower(COALESCE(source,''))<>'nuclei'"),
          'by_kind':rows(r,"SELECT COALESCE(kind,'(missing)') kind,COUNT(*) n,SUM(source_url IS NOT NULL AND trim(source_url)<>'') with_url,SUM(description IS NOT NULL AND trim(description)<>'') with_description,SUM(content IS NOT NULL AND trim(content)<>'') with_content,SUM(versions IS NOT NULL AND trim(versions)<>'') with_versions,SUM(verified=1) legacy_verified FROM poc WHERE lower(COALESCE(source,''))<>'nuclei' GROUP BY kind ORDER BY n DESC"),
          'by_source':rows(r,"SELECT COALESCE(source,'(missing)') source,COUNT(*) n FROM poc WHERE lower(COALESCE(source,''))<>'nuclei' GROUP BY source ORDER BY n DESC"),
          'governance_coverage':one(r,"SELECT COUNT(*) total,COUNT(m.knowledge_id) with_metadata,SUM(m.verification_basis IS NOT NULL AND m.verification_basis<>'') with_verification_basis FROM poc p LEFT JOIN k_meta m ON m.knowledge_id=p.id WHERE lower(COALESCE(p.source,''))<>'nuclei'")
        }
        # Nuclei: source corpus, parser metadata, governance status, never execution validation.
        nuclei="plugin_id='knowledge.nuclei-templates' AND plugin_version=(SELECT version FROM plugin WHERE plugin_id='knowledge.nuclei-templates' AND is_active=1)"
        out['domains']['nuclei_templates']={
          'inventory':one(k,f"SELECT COUNT(*) total,COUNT(DISTINCT item_key) unique_keys,COUNT(DISTINCT content_sha256) unique_content,SUM(review_status='APPROVED') approved,SUM(review_status='DISCOVERED') discovered,SUM(review_status='REJECTED') rejected,SUM(length(body_bytes)>0) body_available,SUM(length(source_commit)>0) pinned_commit,SUM(length(license_status)>0) license_status,SUM(length(source_url)>0) source_url,SUM(length(canonical_url)>0) canonical_url FROM knowledge_source_entry WHERE {nuclei}"),
          'parser_status':rows(k,f"SELECT COALESCE(json_extract(meta_json,'$.nuclei_template.parse_status'),'missing') status,COUNT(*) n FROM knowledge_source_entry WHERE {nuclei} GROUP BY status"),
          'severity':rows(k,f"SELECT COALESCE(json_extract(meta_json,'$.nuclei_template.severity'),'missing') severity,COUNT(*) n FROM knowledge_source_entry WHERE {nuclei} GROUP BY severity ORDER BY n DESC"),
          'protocols':rows(k,f"SELECT value protocol,COUNT(*) n FROM knowledge_source_entry,json_each(meta_json,'$.nuclei_template.protocols') WHERE {nuclei} GROUP BY protocol ORDER BY n DESC"),
          'metadata_coverage':one(k,f"SELECT COUNT(*) total,SUM(json_extract(meta_json,'$.nuclei_template.template_id') IS NOT NULL) template_id,SUM(json_extract(meta_json,'$.nuclei_template.author') IS NOT NULL) author,SUM(json_extract(meta_json,'$.nuclei_template.description') IS NOT NULL) description,SUM(json_extract(meta_json,'$.nuclei_template.matcher_count') IS NOT NULL) matcher_count,SUM(json_extract(meta_json,'$.nuclei_template.references') IS NOT NULL) references_count,SUM(json_extract(meta_json,'$.nuclei_template.classification') IS NOT NULL) classification FROM knowledge_source_entry WHERE {nuclei}"),
          'cve_coverage':one(r,"SELECT COUNT(*) total,SUM(cve IS NOT NULL AND trim(cve)<>'') with_cve,SUM(versions IS NOT NULL AND trim(versions)<>'') with_affected_versions,SUM(description IS NOT NULL AND trim(description)<>'') with_description FROM poc WHERE lower(source)='nuclei'")
        }
        # Tools and Skills
        out['domains']['tools']={
          'inventory':one(p,"SELECT COUNT(*) total,SUM(install_state='installed' AND health_state='ok') installed_ok,SUM(install_state='installed' AND health_state='degraded') installed_degraded,SUM(install_state='missing') missing,SUM(source_system<>'') source_system,SUM(source_pk<>'') source_pk,SUM(provenance_ref<>'') provenance_ref FROM tool"),
          'by_permission':rows(p,"SELECT permission_level,COUNT(*) n FROM tool GROUP BY permission_level ORDER BY permission_level"),
          'versions':rows(p,"SELECT status,health,COUNT(*) n FROM tool_version GROUP BY status,health ORDER BY status,health")
        }
        out['domains']['skills']={
          'inventory':one(p,"SELECT COUNT(*) total,SUM(enabled=1) enabled,SUM(length(trim(COALESCE(body_ref,'')))>0) body_ref,SUM(length(trim(COALESCE(required_tools,'')))>2) required_tools,SUM(length(trim(COALESCE(knowledge_refs,'')))>2) knowledge_refs,SUM(source_system<>'') source_system,SUM(source_pk<>'') source_pk FROM skill"),
          'source_distribution':rows(p,"SELECT source_system,COUNT(*) n FROM skill GROUP BY source_system"),
          'candidate_pipeline':{'candidate_pairs':146,'skills_covered':67,'negative_mentions_excluded':6,'eligible_after_static_checks':120,'approved_mappings':0,'note':'Candidate generator output from the 2026-11-27 read-only batch; all remain candidates pending human review.'}
        }
        out['cross_domain']={
          'review_queue':rows(p,"SELECT target_type,decision,COUNT(*) n FROM review GROUP BY target_type,decision ORDER BY target_type,decision"),
          'agent_registry':one(p,"SELECT (SELECT COUNT(*) FROM agent_definition) agent_definitions,(SELECT COUNT(*) FROM agent_run) agent_runs,(SELECT COUNT(*) FROM workflow_definition) workflow_definitions,(SELECT COUNT(*) FROM workflow) legacy_workflows"),
          'search_index':one(s,"SELECT COUNT(*) documents,(SELECT COUNT(*) FROM search_index_fts) fts_rows FROM search_index_doc")
        }
    return out

def main() -> int:
    ap=argparse.ArgumentParser(description=__doc__)
    for n,path in DEFAULTS.items(): ap.add_argument(f'--{n}-db',type=Path,default=path)
    ap.add_argument('--output',type=Path,help='Write JSON report; stdout otherwise')
    a=ap.parse_args(); result=audit(a.platform_db,a.plugin_db,a.rtis_db,a.search_db)
    rendered=json.dumps(result,ensure_ascii=False,indent=2,default=str)+'\n'
    if a.output: a.output.parent.mkdir(parents=True,exist_ok=True); a.output.write_text(rendered,encoding='utf-8')
    else: print(rendered,end='')
    return 0
if __name__=='__main__': raise SystemExit(main())
