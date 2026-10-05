use super::{Library, model::*, repository};
use crate::error::CoreError;

impl Library {
    pub fn save_collection(&self, request: SaveCollection) -> Result<LibraryChange, CoreError> {
        let name = display_name(&request.name)?;
        let description = request.description.trim().to_owned();
        if description.chars().count() > 280 || description.chars().any(char::is_control) {
            return Err(CoreError::InvalidInput);
        }
        self.access(|db, _| {
            let mut value = if let Some(id) = request.id {
                let mut current = repository::collection(db, &id)?;
                repository::revision(current.revision, request.expected_revision.ok_or(CoreError::InvalidInput)?)?;
                current.revision = next_revision(current.revision)?;
                current
            } else {
                if request.expected_revision.is_some() { return Err(CoreError::InvalidInput); }
                let order: u32 = db.query_row("SELECT coalesce(max(json_extract(payload, '$.order')) + 1, 0) FROM collections", [], |row| row.get(0))?;
                Collection { schema_version: INSTANCE_SCHEMA, id: uuid::Uuid::new_v4().to_string(), revision: 0, name: String::new(), description: String::new(), accent: request.accent, icon: request.icon, order, cover_ref: None }
            };
            value.name = name;
            value.description = description;
            value.accent = request.accent;
            value.icon = request.icon;
            db.execute("INSERT INTO collections(id, payload) VALUES(?1, ?2) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload", rusqlite::params![value.id, serde_json::to_string(&value)?])?;
            repository::change(db, value.id, None)
        })
    }

    pub fn delete_collection(&self, request: RecordRequest) -> Result<LibraryChange, CoreError> {
        self.access(|db, _| {
            let value = repository::collection(db, &request.id)?;
            repository::revision(value.revision, request.expected_revision)?;
            let instances = repository::snapshot(db)?.instances;
            let transaction = db.transaction()?;
            for mut instance in instances
                .into_iter()
                .filter(|instance| instance.collection_id.as_deref() == Some(&request.id))
            {
                instance.collection_id = None;
                instance.revision = next_revision(instance.revision)?;
                instance.updated_at = now();
                repository::update_instance(&transaction, &instance)?;
            }
            transaction.execute("DELETE FROM collections WHERE id = ?1", [&request.id])?;
            transaction.commit()?;
            repository::change(db, request.id, None)
        })
    }
}
