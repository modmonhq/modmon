use crate::domain::remote::SharedPidAppArmor;

use super::super::shared_pid::services_sharing_pid;
use super::support::source;

const EXTENDS: &str = "services:
  base:
    image: example/base
    pid: \"container:elsewhere\"
  child:
    extends: base
    image: example/child
  grandchild:
    extends: {service: child}
  own:
    image: example/own
";

#[test]
fn a_pid_inherited_through_extends_gets_the_shared_pid_profile()
-> Result<(), Box<dyn std::error::Error>> {
    let sources = [source("compose.yaml", EXTENDS)];
    let pinned = services_sharing_pid(&sources, SharedPidAppArmor::Pin)?;
    assert_eq!(
        pinned.into_iter().collect::<Vec<_>>(),
        ["base", "child", "grandchild"]
    );
    assert!(services_sharing_pid(&sources, SharedPidAppArmor::NotConfined)?.is_empty());

    let refused = services_sharing_pid(&sources, SharedPidAppArmor::Missing)
        .err()
        .ok_or("a host without the shared-pid profile")?;
    assert_eq!(refused.code(), "MM_PODMAN_APPARMOR");
    assert!(refused.to_string().contains("service 'base'"), "{refused}");
    Ok(())
}

#[test]
fn a_pid_sharing_service_may_not_choose_mm_containers_even_through_extends()
-> Result<(), Box<dyn std::error::Error>> {
    let wide = EXTENDS.replace(
        "    image: example/base\n",
        "    image: example/base\n    security_opt: [\"apparmor = mm-containers\"]\n",
    );
    let refused = services_sharing_pid(&[source("compose.yaml", &wide)], SharedPidAppArmor::Pin)
        .err()
        .ok_or("mm-containers on a pid-sharing service")?;
    assert!(
        refused.to_string().contains("service 'base'")
            && refused.to_string().contains("apparmor=mm-containers"),
        "{refused}"
    );

    let custom = EXTENDS.replace(
        "    image: example/child\n",
        "    image: example/child\n    security_opt: [\"apparmor=custom\"]\n",
    );
    let pinned = services_sharing_pid(&[source("compose.yaml", &custom)], SharedPidAppArmor::Pin)?;
    assert_eq!(pinned.into_iter().collect::<Vec<_>>(), ["base"]);
    Ok(())
}
