//! `reqly-server admin …`: manage users from the command line, e.g. to
//! create the first administrator or recover a lost password.

use reqly_core::SystemClock;
use reqly_storage::users::UserRepo;
use reqly_storage::{OpenOptions, Storage};

use crate::Config;

pub const USAGE: &str = "\
usage: reqly-server admin create <username>    create an administrator
       reqly-server admin password <username>  set a new password
The password is read from REQLY_ADMIN_PASSWORD or the first line of stdin.";

/// Runs `admin <action> <username>`; returns the message to print.
pub async fn admin(
    config: &Config,
    args: &[String],
    password: Option<String>,
) -> anyhow::Result<String> {
    let [action, username] = args else {
        anyhow::bail!("{USAGE}");
    };
    let password = password
        .map(|p| p.trim_end_matches(['\r', '\n']).to_string())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow::anyhow!("no password given\n{USAGE}"))?;
    let storage = Storage::open(&config.database_path(), OpenOptions::default()).await?;
    let users = UserRepo::new(&storage, &SystemClock);
    match action.as_str() {
        "create" => {
            let user = users.create(username, "", &password, true).await?;
            Ok(format!("created administrator {}", user.username))
        }
        "password" => {
            let user = users
                .find_by_username(username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("no user {username}"))?;
            users.set_password(user.id, &password).await?;
            Ok(format!("password of {} changed", user.username))
        }
        _ => anyhow::bail!("{USAGE}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_admins_and_resets_passwords() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            ..Config::default()
        };
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let out = admin(
            &config,
            &args(&["create", "boss"]),
            Some("boss-password\n".into()),
        )
        .await
        .unwrap();
        assert_eq!(out, "created administrator boss");
        admin(
            &config,
            &args(&["password", "boss"]),
            Some("new-password".into()),
        )
        .await
        .unwrap();
        assert!(
            admin(
                &config,
                &args(&["password", "nobody"]),
                Some("x-password".into())
            )
            .await
            .is_err()
        );
        assert!(
            admin(&config, &args(&["create"]), Some("x".into()))
                .await
                .is_err()
        );
        assert!(admin(&config, &args(&["create", "x"]), None).await.is_err());

        let storage = Storage::open(&config.database_path(), OpenOptions::default())
            .await
            .unwrap();
        let users = UserRepo::new(&storage, &SystemClock);
        users.verify_login("boss", "new-password").await.unwrap();
    }
}
