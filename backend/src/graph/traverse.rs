//! BFS traversal over the `knowledge_graph` table.
//!
//! Iterative SQL loop: each hop expands the frontier of known entity names by
//! fetching all triples where `subject` OR `object` is in the current frontier,
//! then adding newly discovered nodes to the frontier for the next hop.
//!
//! Capped at `max_hops` (config value, default 3). Domain filter is applied at
//! every hop so edges from other domains never expand the frontier.

use super::KnowledgeTriple;
use sqlx::SqlitePool;

pub struct TraverseParams<'a> {
    pub anchor: &'a str,
    pub hops: u8,
    pub max_hops: u8,
    pub domain_id: Option<&'a str>,
}

pub async fn bfs_traverse(
    db: &SqlitePool,
    params: TraverseParams<'_>,
) -> Result<Vec<KnowledgeTriple>, sqlx::Error> {
    let hops = params.hops.min(params.max_hops);
    if hops == 0 || params.anchor.is_empty() {
        return Ok(vec![]);
    }

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut frontier: Vec<String> = vec![params.anchor.to_string()];
    let mut results: Vec<KnowledgeTriple> = Vec::new();

    for _ in 0..hops {
        if frontier.is_empty() {
            break;
        }

        // Build a parameterised IN clause for the current frontier.
        // SQLite does not support array binding, so we use a JSON workaround with json_each.
        let frontier_json = serde_json::to_string(&frontier).unwrap_or_else(|_| "[]".into());

        let rows: Vec<KnowledgeTriple> = if let Some(domain) = params.domain_id {
            sqlx::query_as::<_, KnowledgeTriple>(
                r#"
                SELECT id, subject, predicate, object, domain_id, source, source_id,
                       chunk_index, confidence, created_at
                FROM knowledge_graph
                WHERE domain_id = ?
                  AND (
                    subject IN (SELECT value FROM json_each(?))
                    OR object IN (SELECT value FROM json_each(?))
                  )
                "#,
            )
            .bind(domain)
            .bind(&frontier_json)
            .bind(&frontier_json)
            .fetch_all(db)
            .await?
        } else {
            sqlx::query_as::<_, KnowledgeTriple>(
                r#"
                SELECT id, subject, predicate, object, domain_id, source, source_id,
                       chunk_index, confidence, created_at
                FROM knowledge_graph
                WHERE subject IN (SELECT value FROM json_each(?))
                   OR object IN (SELECT value FROM json_each(?))
                "#,
            )
            .bind(&frontier_json)
            .bind(&frontier_json)
            .fetch_all(db)
            .await?
        };

        let mut next_frontier: Vec<String> = Vec::new();
        for triple in rows {
            if !visited.contains(&triple.id) {
                visited.insert(triple.id.clone());
                // Add newly discovered nodes to the next frontier.
                if !frontier.contains(&triple.subject) && !visited.contains(&triple.subject) {
                    next_frontier.push(triple.subject.clone());
                }
                if !frontier.contains(&triple.object) && !visited.contains(&triple.object) {
                    next_frontier.push(triple.object.clone());
                }
                results.push(triple);
            }
        }
        frontier = next_frontier;
    }

    Ok(results)
}
