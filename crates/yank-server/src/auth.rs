//! Account lifecycle, password hashes and revocable sessions.
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use yank_core::{new_id, now_ts};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub admin: bool,
}
#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub remember: bool,
}
#[derive(Debug, Deserialize)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub admin: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Login {
    pub token: String,
    pub user: User,
    pub expires: i64,
}
pub struct Accounts {
    conn: Connection,
}
impl Accounts {
    pub fn open(path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(3))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,username TEXT UNIQUE NOT NULL,password TEXT NOT NULL,admin INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS sessions(hash TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,expires INTEGER NOT NULL);")?;
        Ok(Self { conn })
    }
    pub fn bootstrap(&self, username: Option<&str>, password: Option<&str>) -> anyhow::Result<()> {
        let count: u64 = self
            .conn
            .query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
        if count == 0 {
            self.create(NewUser {
                username: username
                    .ok_or_else(|| anyhow::anyhow!("Set YANK_ADMIN_USERNAME for first startup"))?
                    .into(),
                password: password
                    .ok_or_else(|| anyhow::anyhow!("Set YANK_ADMIN_PASSWORD for first startup"))?
                    .into(),
                admin: true,
            })?;
        }
        Ok(())
    }
    pub fn create(&self, input: NewUser) -> anyhow::Result<User> {
        let username = input.username.trim();
        anyhow::ensure!(
            !username.is_empty() && username.len() <= 80,
            "Username must contain 1–80 bytes"
        );
        anyhow::ensure!(
            (8..=1024).contains(&input.password.len()),
            "Password must contain 8–1024 bytes"
        );
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        let password = Argon2::default()
            .hash_password(input.password.as_bytes(), &salt)
            .map_err(|_| anyhow::anyhow!("Password hash failed"))?
            .to_string();
        let user = User {
            id: new_id(),
            username: username.into(),
            admin: input.admin,
        };
        self.conn.execute(
            "INSERT INTO users VALUES(?1,?2,?3,?4)",
            params![user.id, user.username, password, user.admin],
        )?;
        Ok(user)
    }
    pub fn login(&self, input: &Credentials) -> anyhow::Result<Login> {
        anyhow::ensure!(
            input.username.len() <= 80 && input.password.len() <= 1024,
            "Invalid credentials"
        );
        let row: Option<(String, String, bool, String)> = self
            .conn
            .query_row(
                "SELECT id,username,admin,password FROM users WHERE username=?1",
                [input.username.trim()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (id, username, admin, password) =
            row.ok_or_else(|| anyhow::anyhow!("Invalid credentials"))?;
        let hash =
            PasswordHash::new(&password).map_err(|_| anyhow::anyhow!("Invalid credentials"))?;
        Argon2::default()
            .verify_password(input.password.as_bytes(), &hash)
            .map_err(|_| anyhow::anyhow!("Invalid credentials"))?;
        let token = format!("{}{}", new_id(), new_id());
        let expires = now_ts() + 30 * 86400;
        self.conn
            .execute("DELETE FROM sessions WHERE expires<?1", [now_ts()])?;
        self.conn.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3)",
            params![digest(&token), id, expires],
        )?;
        Ok(Login {
            token,
            user: User {
                id,
                username,
                admin,
            },
            expires,
        })
    }
    pub fn verify(&self, token: &str) -> anyhow::Result<Option<User>> {
        Ok(self.conn.query_row("SELECT u.id,u.username,u.admin FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.hash=?1 AND s.expires>?2",params![digest(token),now_ts()],|r|Ok(User{id:r.get(0)?,username:r.get(1)?,admin:r.get(2)?})).optional()?)
    }
    pub fn logout(&self, token: &str) -> anyhow::Result<()> {
        self.conn
            .execute("DELETE FROM sessions WHERE hash=?1", [digest(token)])?;
        Ok(())
    }
    pub fn change_password(
        &mut self,
        id: &str,
        old_password: &str,
        new_password: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(old_password.len() <= 1024, "Invalid password");
        let password: String =
            self.conn
                .query_row("SELECT password FROM users WHERE id=?1", [id], |row| {
                    row.get(0)
                })?;
        let hash = PasswordHash::new(&password).map_err(|_| anyhow::anyhow!("Invalid password"))?;
        Argon2::default()
            .verify_password(old_password.as_bytes(), &hash)
            .map_err(|_| anyhow::anyhow!("Invalid password"))?;
        anyhow::ensure!(self.reset_password(id, new_password)?, "Unknown account");
        Ok(())
    }
    pub fn reset_password(&mut self, id: &str, new_password: &str) -> anyhow::Result<bool> {
        anyhow::ensure!(
            (8..=1024).contains(&new_password.len()),
            "Password must contain 8–1024 bytes"
        );
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        let password = Argon2::default()
            .hash_password(new_password.as_bytes(), &salt)
            .map_err(|_| anyhow::anyhow!("Password hash failed"))?
            .to_string();
        // Password replacement and revocation are one durable state transition.
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE users SET password=?1 WHERE id=?2",
            params![password, id],
        )?;
        tx.execute("DELETE FROM sessions WHERE user_id=?1", [id])?;
        tx.commit()?;
        Ok(updated == 1)
    }
    pub fn users(&self) -> anyhow::Result<Vec<User>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,username,admin FROM users ORDER BY username")?;
        Ok(stmt
            .query_map([], |r| {
                Ok(User {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    admin: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn remove(&self, id: &str, current: &str) -> anyhow::Result<()> {
        anyhow::ensure!(id != current, "Cannot remove your own account");
        self.conn.execute("DELETE FROM users WHERE id=?1", [id])?;
        Ok(())
    }
}
fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_and_revocation_roll_back_together() {
        let mut accounts = Accounts::open(std::path::Path::new(":memory:")).unwrap();
        let user = accounts
            .create(NewUser {
                username: "user".into(),
                password: "original-password".into(),
                admin: false,
            })
            .unwrap();
        let credentials = Credentials {
            username: "user".into(),
            password: "original-password".into(),
            remember: false,
        };
        let session = accounts.login(&credentials).unwrap();
        accounts
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_revocation BEFORE DELETE ON sessions
             BEGIN SELECT RAISE(ABORT, 'simulated revocation failure'); END;",
            )
            .unwrap();
        assert!(
            accounts
                .reset_password(&user.id, "replacement-password")
                .is_err()
        );
        assert!(accounts.verify(&session.token).unwrap().is_some());
        assert!(accounts.login(&credentials).is_ok());
        assert!(
            accounts
                .login(&Credentials {
                    username: "user".into(),
                    password: "replacement-password".into(),
                    remember: false,
                })
                .is_err()
        );
    }
}
