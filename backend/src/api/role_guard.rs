use crate::{
    auth::User,
    error::{AppError, Result},
};

pub(crate) fn require_operator(user: &User) -> Result<()> {
    if matches!(user.role.as_str(), "owner" | "admin" | "operator") {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub(crate) fn require_admin(user: &User) -> Result<()> {
    if matches!(user.role.as_str(), "owner" | "admin") {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role: &str) -> User {
        User {
            id: "test-user".into(),
            username: "test-user".into(),
            password_hash: String::new(),
            role: role.into(),
            force_password_change: false,
            totp_enabled: false,
            totp_secret: None,
            created_at: 0,
            updated_at: 0,
            expires_at: None,
        }
    }

    #[test]
    fn operator_guard_is_a_closed_positive_allowlist() {
        for role in ["owner", "admin", "operator"] {
            assert!(require_operator(&user(role)).is_ok(), "{role} should pass");
        }
        for role in ["viewer", "guest", "demo", "member", "future-role", ""] {
            assert!(
                require_operator(&user(role)).is_err(),
                "{role:?} should fail closed"
            );
        }
    }

    #[test]
    fn admin_guard_is_a_closed_positive_allowlist() {
        for role in ["owner", "admin"] {
            assert!(require_admin(&user(role)).is_ok(), "{role} should pass");
        }
        for role in [
            "operator",
            "viewer",
            "guest",
            "demo",
            "member",
            "future-role",
            "",
        ] {
            assert!(
                require_admin(&user(role)).is_err(),
                "{role:?} should fail closed"
            );
        }
    }
}
