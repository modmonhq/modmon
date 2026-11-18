//! A plan's config files, in the words of a plan.

use crate::domain::config_file::{ConfigFileChange, ConfigFileUse};

/// One line per config file: where it is mounted, what it comes from, and
/// whether the plan replaces the service's container because of it.
pub(crate) fn config_file_lines(files: &[ConfigFileUse]) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            let change = match file.change() {
                ConfigFileChange::New => "new".to_owned(),
                ConfigFileChange::Unchanged => "unchanged".to_owned(),
                ConfigFileChange::Changed => {
                    format!("changed, {}/{} is replaced", file.stack, file.service)
                }
            };
            format!(
                "  config file: {}/{} {} from {}{} (version {}): {change}",
                file.stack,
                file.service,
                file.target,
                if file.template { "template " } else { "" },
                file.source,
                file.short_version(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(previous: Option<&str>) -> ConfigFileUse {
        ConfigFileUse {
            stack: "app".to_owned(),
            service: "proxy".to_owned(),
            target: "/etc/nginx/nginx.conf".to_owned(),
            source: "proxy/nginx.conf".to_owned(),
            template: true,
            version: "b".repeat(64),
            previous: previous.map(str::to_owned),
        }
    }

    #[test]
    fn a_plan_says_which_service_a_changed_file_replaces() {
        let older = "a".repeat(64);
        let same = "b".repeat(64);
        assert_eq!(
            config_file_lines(&[file(None), file(Some(&older)), file(Some(&same))]),
            [
                "  config file: app/proxy /etc/nginx/nginx.conf from template proxy/nginx.conf (version bbbbbbbbbbbb): new",
                "  config file: app/proxy /etc/nginx/nginx.conf from template proxy/nginx.conf (version bbbbbbbbbbbb): changed, app/proxy is replaced",
                "  config file: app/proxy /etc/nginx/nginx.conf from template proxy/nginx.conf (version bbbbbbbbbbbb): unchanged",
            ]
        );
    }
}
