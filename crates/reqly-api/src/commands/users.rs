//! Accounts, workspace members and server administration (multi-user
//! server, ADR-010). Access rules are in [`crate::access::RULES`].

use reqly_core::{CoreError, CoreResult, Id, Timestamp};
use reqly_storage::users::{ApiToken, Member, MemberRepo, Role, User, UserRepo};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{AppContext, Empty};
use crate::access;
use crate::registry::{Command, Registry};

pub fn register(r: &mut Registry<AppContext>) {
    r.register::<AccountMe>()
        .register::<AccountUpdate>()
        .register::<AccountChangePassword>()
        .register::<AccountTokens>()
        .register::<AccountCreateToken>()
        .register::<AccountDeleteToken>()
        .register::<MembersList>()
        .register::<MembersSet>()
        .register::<MembersRemove>()
        .register::<MembersLeave>()
        .register::<AdminUsers>()
        .register::<AdminCreateUser>()
        .register::<AdminUpdateUser>()
        .register::<AdminDeleteUser>()
        .register::<AdminWorkspaces>();
}

fn users(ctx: &AppContext) -> UserRepo<'_> {
    UserRepo::new(&ctx.storage, ctx.clock.as_ref())
}

fn members(ctx: &AppContext) -> MemberRepo<'_> {
    MemberRepo::new(&ctx.storage, ctx.clock.as_ref())
}

/// The signed-in user; local callers have no account.
fn me() -> CoreResult<Id> {
    access::current().user_id().ok_or_else(|| {
        CoreError::new(
            "account.unavailable",
            "accounts exist only on a multi-user server",
        )
    })
}

fn workspaces_changed(ctx: &AppContext) {
    ctx.events.publish(crate::Event::new(
        "workspaces.changed",
        super::tree::WorkspacesChanged {},
    ));
}

macro_rules! command {
    ($(#[$doc:meta])* $ty:ident, $name:literal, $input:ty, $output:ty, |$ctx:ident, $inp:ident| $body:block) => {
        $(#[$doc])*
        pub struct $ty;
        impl Command<AppContext> for $ty {
            const NAME: &'static str = $name;
            type Input = $input;
            type Output = $output;
            async fn run($ctx: &AppContext, $inp: $input) -> CoreResult<$output> $body
        }
    };
}

// ----- account --------------------------------------------------------------

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AccountMeOutput {
    /// `null` outside the multi-user server.
    pub user: Option<User>,
    /// The caller's role in each of their workspaces (empty for local
    /// callers, who may do everything).
    pub roles: std::collections::HashMap<Id, Role>,
}

command!(
    AccountMe,
    "account.me",
    Empty,
    AccountMeOutput,
    |ctx, _input| {
        let Some(id) = access::current().user_id() else {
            return Ok(AccountMeOutput {
                user: None,
                roles: Default::default(),
            });
        };
        Ok(AccountMeOutput {
            user: Some(users(ctx).get(id).await?),
            roles: members(ctx).workspaces_of(id).await?,
        })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountUpdateInput {
    pub display_name: String,
}

command!(
    AccountUpdate,
    "account.update",
    AccountUpdateInput,
    User,
    |ctx, input| {
        users(ctx)
            .set_display_name(me()?, &input.display_name)
            .await
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangePasswordInput {
    pub current: String,
    pub new: String,
}

command!(
    /// Changes the password and ends every session, this one included.
    AccountChangePassword,
    "account.changePassword",
    ChangePasswordInput,
    Empty,
    |ctx, input| {
        let id = me()?;
        if !users(ctx).check_password(id, &input.current).await? {
            return Err(CoreError::new("auth.invalid", "the current password is wrong"));
        }
        users(ctx).set_password(id, &input.new).await?;
        Ok(Empty {})
    }
);

command!(
    AccountTokens,
    "account.tokens",
    Empty,
    Vec<ApiToken>,
    |ctx, _input| { users(ctx).list_tokens(me()?).await }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateTokenInput {
    pub name: String,
    /// Days until the token expires; `null` = never.
    #[ts(optional)]
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct CreateTokenOutput {
    pub token: ApiToken,
    /// The token itself, shown once.
    pub secret: String,
}

command!(
    AccountCreateToken,
    "account.createToken",
    CreateTokenInput,
    CreateTokenOutput,
    |ctx, input| {
        let expires = input
            .expires_in_days
            .map(|d| Timestamp(ctx.clock.now().0 + i64::from(d) * 86_400_000));
        let (token, secret) = users(ctx).create_token(me()?, &input.name, expires).await?;
        Ok(CreateTokenOutput { token, secret })
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TokenIdInput {
    pub token_id: Id,
}

command!(
    AccountDeleteToken,
    "account.deleteToken",
    TokenIdInput,
    Empty,
    |ctx, input| {
        users(ctx).delete_token(me()?, input.token_id).await?;
        Ok(Empty {})
    }
);

// ----- members --------------------------------------------------------------

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MembersListInput {
    pub workspace_id: Id,
}

command!(
    MembersList,
    "members.list",
    MembersListInput,
    Vec<Member>,
    |ctx, input| { members(ctx).list(input.workspace_id).await }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MembersSetInput {
    pub workspace_id: Id,
    pub username: String,
    pub role: Role,
}

command!(
    /// Adds a user by name or changes their role.
    MembersSet,
    "members.set",
    MembersSetInput,
    Vec<Member>,
    |ctx, input| {
        let user = users(ctx)
            .find_by_username(&input.username)
            .await?
            .ok_or_else(|| {
                CoreError::new("member.unknown_user", "no such user")
                    .with_param("username", input.username.as_str())
            })?;
        members(ctx)
            .set(input.workspace_id, user.id, input.role)
            .await?;
        workspaces_changed(ctx);
        members(ctx).list(input.workspace_id).await
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MembersRemoveInput {
    pub workspace_id: Id,
    pub user_id: Id,
}

command!(
    MembersRemove,
    "members.remove",
    MembersRemoveInput,
    Vec<Member>,
    |ctx, input| {
        members(ctx)
            .remove(input.workspace_id, input.user_id)
            .await?;
        workspaces_changed(ctx);
        members(ctx).list(input.workspace_id).await
    }
);

command!(
    /// The caller leaves a workspace (the last owner cannot).
    MembersLeave,
    "members.leave",
    MembersListInput,
    Empty,
    |ctx, input| {
        members(ctx).remove(input.workspace_id, me()?).await?;
        workspaces_changed(ctx);
        Ok(Empty {})
    }
);

// ----- administration -------------------------------------------------------

command!(
    AdminUsers,
    "admin.users",
    Empty,
    Vec<User>,
    |ctx, _input| { users(ctx).list().await }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminCreateUserInput {
    pub username: String,
    #[serde(default)]
    #[ts(optional)]
    pub display_name: Option<String>,
    pub password: String,
    #[serde(default)]
    #[ts(optional)]
    pub admin: Option<bool>,
}

command!(
    AdminCreateUser,
    "admin.createUser",
    AdminCreateUserInput,
    User,
    |ctx, input| {
        users(ctx)
            .create(
                &input.username,
                input.display_name.as_deref().unwrap_or_default(),
                &input.password,
                input.admin.unwrap_or(false),
            )
            .await
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminUpdateUserInput {
    pub user_id: Id,
    #[serde(default)]
    #[ts(optional)]
    pub admin: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub disabled: Option<bool>,
    /// A new password (ends the user's sessions).
    #[serde(default)]
    #[ts(optional)]
    pub password: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub display_name: Option<String>,
}

command!(
    AdminUpdateUser,
    "admin.updateUser",
    AdminUpdateUserInput,
    User,
    |ctx, input| {
        let repo = users(ctx);
        let id = input.user_id;
        if let Some(name) = &input.display_name {
            repo.set_display_name(id, name).await?;
        }
        if let Some(password) = &input.password {
            repo.set_password(id, password).await?;
        }
        if let Some(admin) = input.admin {
            repo.set_admin(id, admin).await?;
        }
        if let Some(disabled) = input.disabled {
            repo.set_disabled(id, disabled).await?;
        }
        repo.get(id).await
    }
);

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserIdInput {
    pub user_id: Id,
}

command!(
    AdminDeleteUser,
    "admin.deleteUser",
    UserIdInput,
    Empty,
    |ctx, input| {
        users(ctx).delete(input.user_id).await?;
        workspaces_changed(ctx);
        Ok(Empty {})
    }
);

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminWorkspace {
    pub id: Id,
    pub name: String,
    pub members: Vec<Member>,
}

command!(
    AdminWorkspaces,
    "admin.workspaces",
    Empty,
    Vec<AdminWorkspace>,
    |ctx, _input| {
        let tree = reqly_storage::tree::TreeRepo::new(&ctx.storage, ctx.clock.as_ref());
        let mut out = Vec::new();
        for ws in tree.list_workspaces().await? {
            out.push(AdminWorkspace {
                id: ws.meta.id,
                members: members(ctx).list(ws.meta.id).await?,
                name: ws.name,
            });
        }
        Ok(out)
    }
);
