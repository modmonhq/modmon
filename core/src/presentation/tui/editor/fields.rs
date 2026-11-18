//! The fields of each configuration form, from the resource as it is
//! written today.

use super::*;

fn level_choices() -> Vec<String> {
    std::iter::once(NO_ACCESS.to_owned())
        .chain(LEVELS.iter().map(|level| level.as_str().to_owned()))
        .collect()
}

fn runtime_choices() -> Vec<String> {
    std::iter::once(PROJECT_RUNTIME.to_owned())
        .chain(RUNTIMES.into_iter().map(runtime_name))
        .collect()
}

fn string_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Quotes an argument so `split_command` reads it back unchanged.
fn quote(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '\'' | '"' | '\\'))
    {
        return argument.to_owned();
    }
    format!("'{}'", argument.replace('\'', "'\\''"))
}

fn access_fields(base: &Value, context: &EditorContext) -> Vec<Field> {
    let level = |environment: &str| match base
        .get("access")
        .and_then(|access| access.get(environment))
    {
        Some(Value::String(level)) => level.clone(),
        Some(detailed) => string_at(detailed, "level").to_owned(),
        None => String::new(),
    };
    let help = "none · view: status and logs · operate: + restart and jobs · deploy: + deploy · admin: + exec, forward, secrets";
    let mut environments = context.remote_environments.clone();
    if environments.is_empty() || !level("*").is_empty() {
        environments.insert(0, "*".to_owned());
    }
    environments
        .iter()
        .map(|environment| {
            let current = level(environment);
            Field::choice(
                &format!("access:{environment}"),
                if environment == "*" {
                    "Everywhere"
                } else {
                    environment
                },
                if current.is_empty() {
                    NO_ACCESS
                } else {
                    current.as_str()
                },
                level_choices(),
                help,
            )
        })
        .collect()
}

/// The fields of the form for `kind`. A request to join names the user;
/// `name` becomes that name.
pub(super) fn form(
    kind: EditorKind,
    adding: bool,
    base: &Value,
    context: &EditorContext,
    name: &mut String,
) -> Vec<Field> {
    match kind {
        EditorKind::Environment => environment(adding, base),
        EditorKind::Binding => binding(base),
        EditorKind::Job => job(adding, base),
        EditorKind::Schedule => schedule(adding, base, context),
        EditorKind::Teammate => teammate(adding, base, context),
        EditorKind::Join => {
            *name = context.user.clone().unwrap_or_default();
            join(name, base, context)
        }
    }
}

fn environment(adding: bool, base: &Value) -> Vec<Field> {
    let mut fields = Vec::new();
    if adding {
        fields
            .push(Field::text("name", "Name", "", "For example staging or production.").required());
    }
    fields.push(Field::text(
        "target",
        "Target",
        string_at(base, "target"),
        "Where it runs: a name from `mm target list`. Empty uses the project's target.",
    ));
    let driver = base
        .get("runtime")
        .map_or("", |runtime| string_at(runtime, "driver"));
    fields.push(Field::choice(
        "runtime",
        "Runtime",
        if driver.is_empty() {
            PROJECT_RUNTIME
        } else {
            driver
        },
        runtime_choices(),
        "The container runtime on that target.",
    ));
    fields.push(Field {
        kind: FieldKind::List,
        ..Field::text(
            "deploy-from",
            "Deploys from",
            &base
                .get("deploy-from")
                .and_then(Value::as_array)
                .map(|branches| {
                    branches
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default(),
            "Branches a remote deploy must come from, comma-separated. Empty allows any branch.",
        )
    });
    fields
}

fn binding(base: &Value) -> Vec<Field> {
    let mut fields = Vec::new();
    let secret = base.get("secret").is_some();
    fields.push(Field::choice(
            "kind",
            "Kind",
            if secret { "secret" } else { "value" },
            vec!["value".to_owned(), "secret".to_owned()],
            "A plain value is stored in the configuration. A secret names a declared secret; its material never is.",
        ));
    let current = match base.get("value") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => string_at(base, "secret").to_owned(),
        Some(other) => other.to_string(),
    };
    fields.push(Field::text(
        "value",
        "Value",
        &current,
        "The value, or the name of the secret.",
    ));
    fields
}

fn job(adding: bool, base: &Value) -> Vec<Field> {
    let mut fields = Vec::new();
    if adding {
        fields.push(Field::text("name", "Name", "", "For example migrate or cleanup.").required());
    }
    fields.push(
        Field::text(
            "module",
            "Module",
            string_at(base, "module"),
            "The module whose service runs the job.",
        )
        .required(),
    );
    fields.push(
        Field::text(
            "service",
            "Service",
            string_at(base, "service"),
            "The job runs in a new container of this service.",
        )
        .required(),
    );
    let command = base
        .get("command")
        .and_then(Value::as_array)
        .map(|arguments| {
            arguments
                .iter()
                .filter_map(Value::as_str)
                .map(quote)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    fields.push(
        Field {
            kind: FieldKind::Command,
            ..Field::text(
                "command",
                "Command",
                &command,
                "The exact arguments; quotes group words. No shell runs.",
            )
        }
        .required(),
    );
    fields.push(Field::text(
        "timeout",
        "Timeout",
        string_at(base, "timeout"),
        "For example 30s, 5m, or 1h. Empty uses the default.",
    ));
    fields
}

fn schedule(adding: bool, base: &Value, context: &EditorContext) -> Vec<Field> {
    let mut fields = Vec::new();
    if adding {
        fields.push(Field::text("name", "Name", "", "For example nightly-cleanup.").required());
    }
    let job = string_at(base, "job");
    let help = "The job this schedule runs.";
    fields.push(if context.jobs.is_empty() {
        Field::text("job", "Job", job, help).required()
    } else {
        Field::choice(
            "job",
            "Job",
            if job.is_empty() {
                &context.jobs[0]
            } else {
                job
            },
            context.jobs.clone(),
            help,
        )
    });
    fields.push(
        Field::text(
            "cron",
            "Cron",
            string_at(base, "cron"),
            "Minute, hour, day, month, weekday. For example 0 3 * * * runs daily at 03:00.",
        )
        .required(),
    );
    fields.push(Field::text(
        "timezone",
        "Timezone",
        string_at(base, "timezone"),
        "An IANA name such as Europe/Berlin. Empty uses UTC.",
    ));
    fields.push(Field::toggle(
        "enabled",
        "Enabled",
        base.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        "A disabled schedule stays configured and does not run.",
    ));
    fields
}

fn teammate(adding: bool, base: &Value, context: &EditorContext) -> Vec<Field> {
    let mut fields = Vec::new();
    if adding {
        fields.push(
            Field::text(
                "name",
                "Name",
                "",
                "Lowercase letters, digits, and dashes. For example alice.",
            )
            .required(),
        );
        fields.push(
                Field::text(
                    "key",
                    "Public key",
                    "",
                    "They run `mm key create`, then `mm key show`, and send you the line starting ssh-ed25519.",
                )
                .required(),
            );
    }
    fields.extend(access_fields(base, context));
    if adding {
        fields.push(Field::toggle(
            "machine",
            "Automation",
            false,
            "Yes for a CI or automation identity rather than a person.",
        ));
    }
    fields
}

fn join(name: &str, base: &Value, context: &EditorContext) -> Vec<Field> {
    let mut fields = Vec::new();
    fields.push(
        Field::text(
            "name",
            "Your name",
            name,
            "Lowercase letters, digits, and dashes. This is how the team sees you.",
        )
        .required(),
    );
    fields.extend(access_fields(base, context));
    fields
}

/// The sentence under the form's title, when the kind has one.
pub(super) fn note(kind: EditorKind) -> Option<String> {
    match kind {
        EditorKind::Teammate => Some(
            "The users file in the repo decides who reaches each remote environment. A change takes effect once it is merged and deployed.",
        ),
        EditorKind::Join => Some(
            "Proposes this device's key for the users file. A code owner approves it by merging.",
        ),
        EditorKind::Binding => Some(
            "Stored in the project configuration and shared through the repo. Local overrides still win on your machine.",
        ),
        EditorKind::Environment | EditorKind::Job | EditorKind::Schedule => None,
    }
    .map(str::to_owned)
}
