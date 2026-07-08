use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::CategoryNode;

pub fn list_category_tree(conn: &Connection) -> AppResult<Vec<CategoryNode>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.parent_id,
                (SELECT COUNT(*) FROM repo_categories rc WHERE rc.category_id = c.id) AS count
         FROM categories c
         ORDER BY c.parent_id IS NOT NULL, c.name COLLATE NOCASE",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;

    let mut tops: Vec<CategoryNode> = Vec::new();
    let mut children: Vec<(i64, CategoryNode)> = Vec::new();

    for row in rows {
        let (id, name, parent_id, count) = row?;
        let node = CategoryNode {
            id,
            name,
            parent_id,
            count,
            children: Vec::new(),
        };
        match parent_id {
            None => tops.push(node),
            Some(pid) => children.push((pid, node)),
        }
    }

    for (pid, child) in children {
        if let Some(parent) = tops.iter_mut().find(|n| n.id == pid) {
            parent.children.push(child);
        } else {
            // Orphan subcategory — surface as top-level so it isn't lost.
            tops.push(child);
        }
    }

    Ok(tops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::store::open_and_migrate;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn empty_tree_when_no_categories() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("starboard_cats_{nanos}.db"));
        let conn = open_and_migrate(&path).expect("migrate");
        let tree = list_category_tree(&conn).expect("tree");
        assert!(tree.is_empty());
    }
}
