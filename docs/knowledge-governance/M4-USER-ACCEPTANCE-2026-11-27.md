# M4 user acceptance record — 2026-11-27

## Decision

The user confirmed that they inspected a sample of the 140 cross-source search-result candidates, found the content acceptable, and approved the batch. The user explicitly stated that submitting all 140 individual labels would be too cumbersome and authorized acceptance without those clicks.

## Accepted scope

- The 140-pair batch is accepted for the M4 project milestone under sampled user review.
- No additional per-task Label Studio submissions are required for this acceptance decision.
- M4 implementation work and the associated batch preparation can proceed to closeout with this limitation documented.

## Evidence retained

| Evidence | Observed value |
|---|---:|
| Candidate pairs | 140 |
| Queries | 14 |
| Label Studio tasks | 140 |
| Label Studio predictions | 140 |
| Persisted human annotations | 0 |
| Persisted annotation drafts | 0 |
| CSV human_grade values | 0 / 140 |
| Evaluator state | pending_human_judgment |
| Numerical relevance metrics | null |

## Explicit limitations

This is a sampled user acceptance, not exhaustive per-pair relevance annotation. The AI-suggested grades remain advisory and are not treated as human labels. The existing evaluator must continue to reject incomplete qrels. No NDCG@10, MRR@10, MAP@10, Precision@5/10, Recall or other human-judged relevance metric may be claimed from this batch.

If quantitative ranking evaluation is required later, use a smaller user-approved judged sample or a separate qrel batch. Do not silently fill grades, infer grades from AI predictions, or reopen this batch solely to require 140 clicks.

## Data integrity

The source CSV, Label Studio export, AI suggestion audit and metrics output were not rewritten to manufacture labels. The zero-annotation state is preserved as the factual evidence supporting this waiver.
