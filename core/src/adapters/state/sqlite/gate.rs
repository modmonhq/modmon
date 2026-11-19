use super::*;

use crate::app::ports::{GatePolicySource, GateRepository};
use crate::domain::gate::{
    GateAuditRecord, GatePolicy, GateRouteAttachment, GateSessionMetadata, GateUserMetadata,
    GateUserRecord, PasswordHashRecord,
};

impl GateRepository for SqliteStateStore {
    fn policy(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
    ) -> Result<Option<GatePolicy>, MmError> {
        self.connection()?.query_row(
            "SELECT policy_json FROM gate_policies WHERE project_id=?1 AND environment_name=?2 AND policy_name=?3",
            params![project_id.0.to_string(), environment, name], |row| row.get::<_, String>(0),
        ).optional().map_err(state_error)?.map(|json| decode(&self.path, &json, "gate policy")).transpose()
    }

    fn policy_by_id(&self, id: Uuid) -> Result<Option<GatePolicy>, MmError> {
        self.connection()?
            .query_row(
                "SELECT policy_json FROM gate_policies WHERE id=?1",
                [id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(state_error)?
            .map(|json| decode(&self.path, &json, "gate policy"))
            .transpose()
    }

    fn list_policies(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Vec<GatePolicy>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT policy_json FROM gate_policies WHERE project_id=?1 AND environment_name=?2 ORDER BY policy_name").map_err(state_error)?;
        let rows = statement
            .query_map(params![project_id.0.to_string(), environment], |row| {
                row.get::<_, String>(0)
            })
            .map_err(state_error)?;
        rows.map(|row| {
            row.map_err(state_error)
                .and_then(|json| decode(&self.path, &json, "gate policy"))
        })
        .collect()
    }

    fn insert_policy(&self, policy: &GatePolicy) -> Result<(), MmError> {
        policy
            .validate()
            .map_err(|message| MmError::Security { message })?;
        self.connection()?.execute(
            "INSERT INTO gate_policies(id,project_id,environment_name,policy_name,policy_json,revision,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![policy.id.to_string(), policy.project_id.0.to_string(), policy.environment, policy.name.as_str(), encode(policy, "gate policy")?, i64::try_from(policy.revision).unwrap_or(i64::MAX), policy.updated_at_unix_ms],
        ).map_err(map_unique("a gate policy with that name already exists"))?;
        Ok(())
    }

    fn replace_policy(&self, policy: &GatePolicy, expected_revision: u64) -> Result<(), MmError> {
        policy
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let changed = self.connection()?.execute(
            "UPDATE gate_policies SET policy_json=?1,revision=?2,updated_at_ms=?3 WHERE id=?4 AND revision=?5",
            params![encode(policy, "gate policy")?, i64::try_from(policy.revision).unwrap_or(i64::MAX), policy.updated_at_unix_ms, policy.id.to_string(), i64::try_from(expected_revision).unwrap_or(i64::MAX)],
        ).map_err(state_error)?;
        if changed == 1 {
            Ok(())
        } else {
            Err(stale("gate policy"))
        }
    }

    fn remove_policy(&self, id: Uuid, expected_revision: u64) -> Result<bool, MmError> {
        self.connection()?
            .execute(
                "DELETE FROM gate_policies WHERE id=?1 AND revision=?2",
                params![
                    id.to_string(),
                    i64::try_from(expected_revision).unwrap_or(i64::MAX)
                ],
            )
            .map(|count| count == 1)
            .map_err(state_error)
    }

    fn user(
        &self,
        policy_id: Uuid,
        normalized_username: &str,
    ) -> Result<Option<GateUserRecord>, MmError> {
        self.connection()?.query_row(
            "SELECT id,policy_id,normalized_username,display_username,password_hash,password_parameter_version,enabled,password_version,failed_attempts,window_started_at_ms,locked_until_ms,created_at_ms,updated_at_ms FROM gate_users WHERE policy_id=?1 AND normalized_username=?2",
            params![policy_id.to_string(), normalized_username], user_from_row,
        ).optional().map_err(state_error)
    }

    fn user_by_id(&self, id: Uuid) -> Result<Option<GateUserRecord>, MmError> {
        self.connection()?.query_row(
            "SELECT id,policy_id,normalized_username,display_username,password_hash,password_parameter_version,enabled,password_version,failed_attempts,window_started_at_ms,locked_until_ms,created_at_ms,updated_at_ms FROM gate_users WHERE id=?1",
            [id.to_string()], user_from_row,
        ).optional().map_err(state_error)
    }

    fn list_users(&self, policy_id: Uuid) -> Result<Vec<GateUserMetadata>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id,policy_id,normalized_username,display_username,password_hash,password_parameter_version,enabled,password_version,failed_attempts,window_started_at_ms,locked_until_ms,created_at_ms,updated_at_ms FROM gate_users WHERE policy_id=?1 ORDER BY normalized_username").map_err(state_error)?;
        statement
            .query_map([policy_id.to_string()], user_from_row)
            .map_err(state_error)?
            .map(|row| row.map(|user| user.metadata).map_err(state_error))
            .collect()
    }

    fn insert_user(&self, user: &GateUserRecord) -> Result<(), MmError> {
        let value = &user.metadata;
        self.connection()?.execute(
            "INSERT INTO gate_users(id,policy_id,normalized_username,display_username,password_hash,password_parameter_version,enabled,password_version,failed_attempts,window_started_at_ms,locked_until_ms,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![value.id.to_string(), value.policy_id.to_string(), value.normalized_username, value.display_username, user.password_hash.encoded, i64::from(user.password_hash.parameter_version), value.enabled, i64::try_from(value.password_version).unwrap_or(i64::MAX), i64::from(user.failed_attempts), user.window_started_at_unix_ms, user.locked_until_unix_ms, value.created_at_unix_ms, value.updated_at_unix_ms],
        ).map_err(map_unique("a user with that normalized username already exists"))?;
        Ok(())
    }

    fn replace_user(
        &self,
        user: &GateUserRecord,
        expected_password_version: u64,
    ) -> Result<(), MmError> {
        let value = &user.metadata;
        let changed = self.connection()?.execute(
            "UPDATE gate_users SET password_hash=?1,password_parameter_version=?2,enabled=?3,password_version=?4,failed_attempts=?5,window_started_at_ms=?6,locked_until_ms=?7,updated_at_ms=?8 WHERE id=?9 AND password_version=?10",
            params![user.password_hash.encoded, i64::from(user.password_hash.parameter_version), value.enabled, i64::try_from(value.password_version).unwrap_or(i64::MAX), i64::from(user.failed_attempts), user.window_started_at_unix_ms, user.locked_until_unix_ms, value.updated_at_unix_ms, value.id.to_string(), i64::try_from(expected_password_version).unwrap_or(i64::MAX)],
        ).map_err(state_error)?;
        if changed == 1 {
            Ok(())
        } else {
            Err(stale("gate user"))
        }
    }

    fn remove_user(
        &self,
        policy_id: Uuid,
        normalized_username: &str,
        expected_password_version: u64,
    ) -> Result<bool, MmError> {
        self.connection()?
            .execute(
                "DELETE FROM gate_users WHERE policy_id=?1 AND normalized_username=?2 AND password_version=?3",
                params![
                    policy_id.to_string(),
                    normalized_username,
                    i64::try_from(expected_password_version).unwrap_or(i64::MAX)
                ],
            )
            .map(|count| count == 1)
            .map_err(state_error)
    }

    fn record_authentication_failure(&self, user_id: Uuid, now: i64) -> Result<(), MmError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        let state = transaction
            .query_row(
                "SELECT failed_attempts,window_started_at_ms FROM gate_users WHERE id=?1",
                [user_id.to_string()],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(state_error)?;
        if let Some((attempts, started)) = state {
            let (attempts, started) = if now.saturating_sub(started) > 15 * 60 * 1000 {
                (1, now)
            } else {
                (attempts.saturating_add(1), started)
            };
            let locked = (attempts >= 5)
                .then(|| now.saturating_add(i64::from(attempts.min(12) - 4) * 30_000));
            transaction.execute("UPDATE gate_users SET failed_attempts=?1,window_started_at_ms=?2,locked_until_ms=?3,updated_at_ms=?4 WHERE id=?5", params![attempts, started, locked, now, user_id.to_string()]).map_err(state_error)?;
        }
        transaction.commit().map_err(state_error)
    }

    fn clear_authentication_failures(&self, user_id: Uuid, now: i64) -> Result<(), MmError> {
        self.connection()?.execute("UPDATE gate_users SET failed_attempts=0,window_started_at_ms=?1,locked_until_ms=NULL,updated_at_ms=?1 WHERE id=?2", params![now, user_id.to_string()]).map_err(state_error)?;
        Ok(())
    }

    fn insert_session(&self, session: &GateSessionMetadata) -> Result<(), MmError> {
        self.connection()?.execute("INSERT INTO gate_sessions(id,policy_id,user_id,username,issued_at_ms,expires_at_ms,last_seen_at_ms,key_version,revoked_at_ms,replaced_by) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![session.id.to_string(), session.policy_id.to_string(), session.user_id.to_string(), session.username, session.issued_at_unix_ms, session.expires_at_unix_ms, session.last_seen_at_unix_ms, session.key_version, session.revoked_at_unix_ms, session.replaced_by.map(|id| id.to_string())]).map_err(state_error)?;
        Ok(())
    }

    fn insert_session_replacing(
        &self,
        session: &GateSessionMetadata,
        replaced_session_id: Uuid,
        now: i64,
    ) -> Result<(), MmError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        transaction.execute("INSERT INTO gate_sessions(id,policy_id,user_id,username,issued_at_ms,expires_at_ms,last_seen_at_ms,key_version,revoked_at_ms,replaced_by) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![session.id.to_string(), session.policy_id.to_string(), session.user_id.to_string(), session.username, session.issued_at_unix_ms, session.expires_at_unix_ms, session.last_seen_at_unix_ms, session.key_version, session.revoked_at_unix_ms, session.replaced_by.map(|id| id.to_string())]).map_err(state_error)?;
        let revoked = transaction.execute(
            "UPDATE gate_sessions SET revoked_at_ms=?1,replaced_by=?2 WHERE id=?3 AND policy_id=?4 AND revoked_at_ms IS NULL",
            params![now, session.id.to_string(), replaced_session_id.to_string(), session.policy_id.to_string()],
        ).map_err(state_error)?;
        if revoked != 1 {
            return Err(stale("gate session"));
        }
        transaction.commit().map_err(state_error)
    }

    fn session(&self, id: Uuid) -> Result<Option<GateSessionMetadata>, MmError> {
        self.connection()?.query_row("SELECT id,policy_id,user_id,username,issued_at_ms,expires_at_ms,last_seen_at_ms,key_version,revoked_at_ms,replaced_by FROM gate_sessions WHERE id=?1", [id.to_string()], session_from_row).optional().map_err(state_error)
    }

    fn list_sessions(
        &self,
        policy_id: Uuid,
        now: i64,
    ) -> Result<Vec<GateSessionMetadata>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id,policy_id,user_id,username,issued_at_ms,expires_at_ms,last_seen_at_ms,key_version,revoked_at_ms,replaced_by FROM gate_sessions WHERE policy_id=?1 AND expires_at_ms>=?2 ORDER BY issued_at_ms DESC").map_err(state_error)?;
        statement
            .query_map(params![policy_id.to_string(), now], session_from_row)
            .map_err(state_error)?
            .map(|row| row.map_err(state_error))
            .collect()
    }

    fn revoke_session(
        &self,
        id: Uuid,
        now: i64,
        replaced_by: Option<Uuid>,
    ) -> Result<bool, MmError> {
        self.connection()?.execute("UPDATE gate_sessions SET revoked_at_ms=?1,replaced_by=?2 WHERE id=?3 AND revoked_at_ms IS NULL", params![now, replaced_by.map(|id| id.to_string()), id.to_string()]).map(|count| count == 1).map_err(state_error)
    }
    fn revoke_user_sessions(&self, user_id: Uuid, now: i64) -> Result<u64, MmError> {
        self.connection()?.execute("UPDATE gate_sessions SET revoked_at_ms=?1 WHERE user_id=?2 AND revoked_at_ms IS NULL", params![now, user_id.to_string()]).map(|count| u64::try_from(count).unwrap_or(u64::MAX)).map_err(state_error)
    }

    fn attachment(
        &self,
        target_id: TargetId,
        router_name: &str,
    ) -> Result<Option<GateRouteAttachment>, MmError> {
        self.connection()?.query_row("SELECT policy_id,target_id,project_id,environment_name,router_name,host,path_prefix,middleware_name,attached_at_ms FROM gate_route_attachments WHERE target_id=?1 AND router_name=?2", params![target_id.0.to_string(), router_name], attachment_from_row).optional().map_err(state_error)
    }
    fn list_attachments(&self, policy_id: Uuid) -> Result<Vec<GateRouteAttachment>, MmError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT policy_id,target_id,project_id,environment_name,router_name,host,path_prefix,middleware_name,attached_at_ms FROM gate_route_attachments WHERE policy_id=?1 ORDER BY target_id,router_name").map_err(state_error)?;
        statement
            .query_map([policy_id.to_string()], attachment_from_row)
            .map_err(state_error)?
            .map(|row| row.map_err(state_error))
            .collect()
    }
    fn insert_attachment(&self, value: &GateRouteAttachment) -> Result<(), MmError> {
        self.connection()?.execute("INSERT INTO gate_route_attachments(policy_id,target_id,project_id,environment_name,router_name,host,path_prefix,middleware_name,attached_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![value.policy_id.to_string(), value.target_id.0.to_string(), value.project_id.0.to_string(), value.environment, value.router_name, value.host, value.path_prefix, value.middleware_name, value.attached_at_unix_ms]).map_err(map_unique("the route is already attached to a gate"))?;
        Ok(())
    }
    fn remove_attachment(
        &self,
        target_id: TargetId,
        router_name: &str,
        policy_id: Uuid,
    ) -> Result<bool, MmError> {
        self.connection()?.execute("DELETE FROM gate_route_attachments WHERE target_id=?1 AND router_name=?2 AND policy_id=?3", params![target_id.0.to_string(), router_name, policy_id.to_string()]).map(|count| count == 1).map_err(state_error)
    }

    fn consume_cap_token(
        &self,
        policy_id: Uuid,
        digest: &str,
        now: i64,
        expires: i64,
    ) -> Result<bool, MmError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(state_error)?;
        transaction
            .execute(
                "DELETE FROM gate_cap_redemptions WHERE expires_at_ms<?1",
                [now],
            )
            .map_err(state_error)?;
        let inserted = transaction.execute("INSERT OR IGNORE INTO gate_cap_redemptions(token_digest,policy_id,redeemed_at_ms,expires_at_ms) VALUES(?1,?2,?3,?4)", params![digest, policy_id.to_string(), now, expires]).map_err(state_error)? == 1;
        transaction.commit().map_err(state_error)?;
        Ok(inserted)
    }

    fn append_audit(&self, record: &GateAuditRecord) -> Result<(), MmError> {
        self.connection()?.execute(
            "INSERT INTO gate_audit(id,action,project_id,environment_name,policy_name,subject,plan_hash,occurred_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![record.id.to_string(), record.action.as_str(), record.project_id.0.to_string(), record.environment, record.policy, record.subject, record.plan_hash, record.occurred_at_unix_ms],
        ).map_err(state_error)?;
        Ok(())
    }
}

impl GatePolicySource for SqliteStateStore {
    fn gate_components_required(&self) -> Result<bool, MmError> {
        self.connection()?
            .query_row("SELECT EXISTS(SELECT 1 FROM gate_policies)", [], |row| {
                row.get(0)
            })
            .map_err(state_error)
    }
}

fn user_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GateUserRecord> {
    let password_version = row.get::<_, i64>(7)?;
    Ok(GateUserRecord {
        metadata: GateUserMetadata {
            id: parse_uuid(row.get::<_, String>(0)?, 0)?,
            policy_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
            normalized_username: row.get(2)?,
            display_username: row.get(3)?,
            enabled: row.get(6)?,
            password_version: u64::try_from(password_version).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })?,
            created_at_unix_ms: row.get(11)?,
            updated_at_unix_ms: row.get(12)?,
        },
        password_hash: PasswordHashRecord {
            encoded: row.get(4)?,
            parameter_version: row.get(5)?,
        },
        failed_attempts: row.get(8)?,
        window_started_at_unix_ms: row.get(9)?,
        locked_until_unix_ms: row.get(10)?,
    })
}
fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GateSessionMetadata> {
    Ok(GateSessionMetadata {
        id: parse_uuid(row.get::<_, String>(0)?, 0)?,
        policy_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
        user_id: parse_uuid(row.get::<_, String>(2)?, 2)?,
        username: row.get(3)?,
        issued_at_unix_ms: row.get(4)?,
        expires_at_unix_ms: row.get(5)?,
        last_seen_at_unix_ms: row.get(6)?,
        key_version: row.get(7)?,
        revoked_at_unix_ms: row.get(8)?,
        replaced_by: row
            .get::<_, Option<String>>(9)?
            .map(|value| parse_uuid(value, 9))
            .transpose()?,
    })
}
fn attachment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GateRouteAttachment> {
    Ok(GateRouteAttachment {
        policy_id: parse_uuid(row.get::<_, String>(0)?, 0)?,
        target_id: TargetId(parse_uuid(row.get::<_, String>(1)?, 1)?),
        project_id: ProjectId(parse_uuid(row.get::<_, String>(2)?, 2)?),
        environment: row.get(3)?,
        router_name: row.get(4)?,
        host: row.get(5)?,
        path_prefix: row.get(6)?,
        middleware_name: row.get(7)?,
        attached_at_unix_ms: row.get(8)?,
    })
}
fn parse_uuid(value: String, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn encode(value: &impl serde::Serialize, kind: &str) -> Result<String, MmError> {
    serde_json::to_string(value).map_err(|error| MmError::State {
        message: format!("could not serialize {kind}: {error}"),
    })
}
fn decode<T: serde::de::DeserializeOwned>(
    path: &Path,
    json: &str,
    kind: &str,
) -> Result<T, MmError> {
    stored_json(path, json, kind)
}
fn stale(kind: &str) -> MmError {
    MmError::State {
        message: format!("{kind} changed after review"),
    }
}
fn map_unique(message: &'static str) -> impl FnOnce(rusqlite::Error) -> MmError {
    move |error| {
        if matches!(error, rusqlite::Error::SqliteFailure(ref code, _) if code.extended_code == 2067 || code.extended_code == 1555)
        {
            MmError::State {
                message: message.to_owned(),
            }
        } else {
            state_error(error)
        }
    }
}
