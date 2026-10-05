use crate::{
    error::CoreError,
    game::local_profile::{offline_uuid, valid_nickname},
    instances::{
        model::{next_revision, valid_id},
        repository,
    },
    settings::Settings,
    storage::Database,
};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalProfile {
    pub id: String,
    pub nickname: String,
    pub offline_uuid: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    pub profiles: Vec<LocalProfile>,
    pub active_profile_id: String,
    pub revision: u32,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProfileAction {
    Create,
    Rename,
    Delete,
    Select,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditProfile {
    pub action: ProfileAction,
    pub id: Option<String>,
    pub nickname: Option<String>,
    pub expected_revision: u32,
}

fn initialize(db: &mut Connection) -> Result<(), CoreError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM profile_state WHERE id=1)",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        let payload: String =
            tx.query_row("SELECT payload FROM settings WHERE id=1", [], |r| r.get(0))?;
        let settings: Settings = serde_json::from_str(&payload)?;
        settings.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO local_profiles(id,nickname) VALUES(?1,?2)",
            rusqlite::params![id, settings.local_nickname],
        )?;
        tx.execute(
            "INSERT INTO profile_state(id,revision,active_id) VALUES(1,0,?1)",
            [&id],
        )?;
    }
    tx.commit()?;
    Ok(())
}
fn read(db: &Connection) -> Result<ProfileSnapshot, CoreError> {
    let (revision, active_profile_id) = db.query_row(
        "SELECT revision,active_id FROM profile_state WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut query = db.prepare("SELECT id,nickname FROM local_profiles ORDER BY rowid")?;
    let profiles = query
        .query_map([], |r| {
            let nickname: String = r.get(1)?;
            Ok(LocalProfile {
                id: r.get(0)?,
                offline_uuid: offline_uuid(&nickname).to_string(),
                nickname,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProfileSnapshot {
        profiles,
        active_profile_id,
        revision,
    })
}
pub fn snapshot(database: &Database) -> Result<ProfileSnapshot, CoreError> {
    let mut db = database.connect()?;
    initialize(&mut db)?;
    let tx = db.transaction()?;
    let state = read(&tx)?;
    tx.commit()?;
    Ok(state)
}
pub fn edit(database: &Database, request: EditProfile) -> Result<ProfileSnapshot, CoreError> {
    let mut db = database.connect()?;
    initialize(&mut db)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let state = read(&tx)?;
    repository::revision(state.revision, request.expected_revision)?;
    let mut active = state.active_profile_id.clone();
    match request.action {
        ProfileAction::Create | ProfileAction::Rename => {
            let name = request.nickname.as_deref().ok_or(CoreError::InvalidInput)?;
            if !valid_nickname(name)
                || state.profiles.len() >= 100 && matches!(request.action, ProfileAction::Create)
            {
                return Err(CoreError::InvalidInput);
            }
            if state.profiles.iter().any(|p| {
                p.nickname.eq_ignore_ascii_case(name) && Some(&p.id) != request.id.as_ref()
            }) {
                return Err(CoreError::InvalidInput);
            }
            if matches!(request.action, ProfileAction::Create) {
                if request.id.is_some() {
                    return Err(CoreError::InvalidInput);
                }
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO local_profiles(id,nickname) VALUES(?1,?2)",
                    rusqlite::params![id, name],
                )?;
            } else {
                let id = request.id.as_deref().ok_or(CoreError::InvalidInput)?;
                if !state.profiles.iter().any(|p| p.id == id) {
                    return Err(CoreError::NotFound);
                }
                tx.execute(
                    "UPDATE local_profiles SET nickname=?2 WHERE id=?1",
                    rusqlite::params![id, name],
                )?;
            }
        }
        ProfileAction::Delete | ProfileAction::Select => {
            let id = request.id.as_deref().ok_or(CoreError::InvalidInput)?;
            valid_id(id)?;
            if !state.profiles.iter().any(|p| p.id == id) {
                return Err(CoreError::NotFound);
            }
            if matches!(request.action, ProfileAction::Select) {
                active = id.into();
            } else {
                if state.profiles.len() == 1 {
                    return Err(CoreError::InvalidInput);
                }
                if active == id {
                    active = state
                        .profiles
                        .iter()
                        .find(|p| p.id != id)
                        .unwrap()
                        .id
                        .clone();
                }
                tx.execute(
                    "UPDATE profile_state SET active_id=?1 WHERE id=1",
                    [&active],
                )?;
                tx.execute("DELETE FROM local_profiles WHERE id=?1", [id])?;
            }
        }
    }
    tx.execute(
        "UPDATE profile_state SET revision=?1,active_id=?2 WHERE id=1",
        rusqlite::params![next_revision(state.revision)?, active],
    )?;
    // Keep the previous single-nickname setting compatible with existing users/tools.
    let nickname: String = tx.query_row(
        "SELECT nickname FROM local_profiles WHERE id=?1",
        [&active],
        |r| r.get(0),
    )?;
    let payload: String =
        tx.query_row("SELECT payload FROM settings WHERE id=1", [], |r| r.get(0))?;
    let mut settings: Settings = serde_json::from_str(&payload)?;
    settings.local_nickname = nickname;
    tx.execute(
        "UPDATE settings SET revision=revision+1,payload=?1 WHERE id=1",
        [serde_json::to_string(&settings)?],
    )?;
    let result = read(&tx)?;
    tx.commit()?;
    Ok(result)
}
pub fn active(database: &Database) -> Result<LocalProfile, CoreError> {
    let state = snapshot(database)?;
    state
        .profiles
        .into_iter()
        .find(|p| p.id == state.active_profile_id)
        .ok_or(CoreError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_or_renaming_global_profile_does_not_change_instances_or_worlds() {
        use crate::instances::{
            Library,
            model::{CreateInstance, Loader},
        };
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("shared library");
        let db = Database::new(root.join("launcher/sporium.sqlite3"));
        let library = Library::new(root.clone(), db.clone());
        let created = library
            .create(CreateInstance {
                name: "Shared world".into(),
                minecraft_version: "1.21.1".into(),
                loader: Loader::Vanilla,
                collection_id: None,
            })
            .unwrap();
        let before = serde_json::to_string(&library.snapshot().unwrap()).unwrap();
        let world = root
            .join("instances")
            .join(created.affected_id)
            .join("saves/example.dat");
        std::fs::write(&world, b"world data").unwrap();
        let first = snapshot(&db).unwrap();
        let second = edit(
            &db,
            EditProfile {
                action: ProfileAction::Create,
                id: None,
                nickname: Some("AnotherNick".into()),
                expected_revision: first.revision,
            },
        )
        .unwrap();
        let selected = edit(
            &db,
            EditProfile {
                action: ProfileAction::Select,
                id: Some(second.profiles[1].id.clone()),
                nickname: None,
                expected_revision: second.revision,
            },
        )
        .unwrap();
        edit(
            &db,
            EditProfile {
                action: ProfileAction::Rename,
                id: Some(selected.active_profile_id),
                nickname: Some("AnyLocalName".into()),
                expected_revision: selected.revision,
            },
        )
        .unwrap();
        assert_eq!(active(&db).unwrap().nickname, "AnyLocalName");
        assert_eq!(
            serde_json::to_string(&library.snapshot().unwrap()).unwrap(),
            before
        );
        assert_eq!(std::fs::read(world).unwrap(), b"world data");
    }
    #[test]
    fn profiles_migrate_switch_validate_conflicts_and_preserve_last_identity() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("test.sqlite"));
        let first = snapshot(&db).unwrap();
        assert_eq!(first.profiles[0].nickname, "SporiumLocal");
        assert_eq!(
            snapshot(&db).unwrap().active_profile_id,
            first.active_profile_id
        );
        let second = edit(
            &db,
            EditProfile {
                action: ProfileAction::Create,
                id: None,
                nickname: Some("Alex".into()),
                expected_revision: 0,
            },
        )
        .unwrap();
        assert!(
            edit(
                &db,
                EditProfile {
                    action: ProfileAction::Create,
                    id: None,
                    nickname: Some("Another".into()),
                    expected_revision: 0
                }
            )
            .is_err()
        );
        let id = second.profiles[1].id.clone();
        let selected = edit(
            &db,
            EditProfile {
                action: ProfileAction::Select,
                id: Some(id.clone()),
                nickname: None,
                expected_revision: 1,
            },
        )
        .unwrap();
        assert_eq!(selected.active_profile_id, id);
        assert_eq!(db.load_settings().unwrap().values.local_nickname, "Alex");
        let deleted = edit(
            &db,
            EditProfile {
                action: ProfileAction::Delete,
                id: Some(id),
                nickname: None,
                expected_revision: 2,
            },
        )
        .unwrap();
        assert_eq!(deleted.profiles.len(), 1);
        assert!(
            edit(
                &db,
                EditProfile {
                    action: ProfileAction::Delete,
                    id: Some(deleted.active_profile_id),
                    nickname: None,
                    expected_revision: 3
                }
            )
            .is_err()
        );
    }
}
