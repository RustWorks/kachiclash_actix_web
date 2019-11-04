use rusqlite::{Row, Connection, OptionalExtension};
use chrono::{DateTime, Utc};

use crate::external::{self, discord};
use super::{Award, DataError};

pub type PlayerId = i64;

#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub join_date: DateTime<Utc>,
    admin_level: u8,
    pub discord_info: Option<discord::DiscordUserInfo>,
}

impl Player {
    pub fn with_id(db: &Connection, player_id: PlayerId) -> Result<Option<Self>, DataError> {
        db.query_row("
                SELECT
                    p.id, p.name, p.join_date, p.admin_level,
                    d.user_id, d.username, d.avatar, d.discriminator,
                    COALESCE((
                        SELECT 1
                        FROM award AS a
                        WHERE a.player_id = p.id AND type = ?
                        LIMIT 1
                    ), 0) AS has_emperors_cup
                FROM player AS p
                LEFT JOIN player_discord AS d ON d.player_id = p.id
                WHERE p.id = ?
            ", params![Award::EmperorsCup, player_id], |row| Player::from_row(row))
            .optional()
            .map_err(|e| e.into())
    }

    pub fn list_all(db: &Connection) -> Result<Vec<Self>, DataError> {
        db.prepare("
                SELECT
                    p.id, p.name, p.join_date, p.admin_level,
                    d.user_id, d.username, d.avatar, d.discriminator,
                    COALESCE((
                        SELECT 1
                        FROM award AS a
                        WHERE a.player_id = p.id AND type = ?
                        LIMIT 1
                    ), 0) AS has_emperors_cup
                FROM player AS p
                LEFT JOIN player_discord AS d ON d.player_id = p.id
            ").unwrap()
            .query_map(params![Award::EmperorsCup], |row| Player::from_row(row))
            .and_then(|mapped_rows| {
                Ok(mapped_rows.map(|r| r.unwrap()).collect::<Vec<Player>>())
            })
            .map_err(|e| e.into())
    }

    pub fn from_row(row: &Row) -> Result<Self, rusqlite::Error> {
        let mut name: String = row.get("name")?;
        let has_emperors_cup: bool = row.get("has_emperors_cup")?;
        if has_emperors_cup {
            name.push_str(" ");
            name.push_str(Award::EmperorsCup.emoji());
        }
        Ok(Player {
            id: row.get("id")?,
            name: name,
            join_date: row.get("join_date")?,
            admin_level: row.get("admin_level")?,
            discord_info: match row.get("user_id")? {
                Some(user_id) => Some(discord::DiscordUserInfo {
                    id: user_id,
                    username: row.get("username")?,
                    avatar: row.get("avatar")?,
                    discriminator: row.get("discriminator")?,
                }),
                None => None
            }
        })
    }

    pub fn is_admin(&self) -> bool {
        self.admin_level > 0
    }

    pub fn tiny_thumb(&self) -> String {
        match &self.discord_info {
            Some(info) => discord::avatar_url(&info, discord::ImageExt::PNG, discord::ImageSize::TINY).to_string(),
            None => "/static/default_avatar.png".to_string(),
        }
    }

    pub fn url_path(&self) -> String {
        format!("/player/{}", self.id)
    }
}

pub fn player_id_with_external_user(db: &mut Connection, user_info: impl external::UserInfo) -> Result<PlayerId, rusqlite::Error> {
    let txn = db.transaction()?;
    let now = Utc::now();
    let existing_player = user_info.update_existing_player(&txn, now)?;
    match existing_player {
        None => {
            txn.execute("INSERT INTO player (join_date, name) VALUES (?, ?)",
                        params![now, user_info.name_suggestion()]).unwrap();
            let player_id = txn.last_insert_rowid();
            user_info.insert_into_db(&txn, now, player_id)?;
            txn.commit()?;
            Ok(player_id)
        },
        Some(player_id) => {
            txn.commit()?;
            Ok(player_id)
        }
    }
}
