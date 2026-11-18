//! A rollback puts the stack files of the last deployment back with the
//! environment of the plan at hand.
//!
//! Found on the test VMs: a deployment interrupted half way on a node was
//! followed by one whose stack had a service with variables (`api`) that the
//! last deployment's files on that node did not define. Recovering the
//! interrupted one put those files back with the new plan's environment
//! overlay; Compose refused with "service \"api\" has neither an image nor a
//! build context specified", the recovery failed, and every later
//! deployment on the node failed the same way.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use mm::adapters::environment::limited_overlays;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn names(text: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(text)?;
    Ok(value["services"]
        .as_mapping()
        .ok_or("the overlay has no services")?
        .keys()
        .filter_map(|key| key.as_str().map(str::to_owned))
        .collect())
}

#[test]
fn an_overlay_that_names_a_service_the_earlier_files_lack_is_limited_to_theirs() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let root = home.path().join("run").join("operation");
    fs::create_dir_all(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let overlay = root.join("app.compose.yaml");
    fs::write(
        &overlay,
        "services:\n  web:\n    environment: {GREETING: hi}\n  whoami:\n    environment: {GREETING: hi}\n  api:\n    environment: {GREETING: hi}\n",
    )?;
    // A stack's own file, outside the bundle, is never rewritten.
    let own = home.path().join("user.000.compose.yaml");
    fs::write(&own, "services:\n  web: {image: nginx}\n")?;
    let earlier = BTreeSet::from(["web".to_owned(), "whoami".to_owned()]);

    let limited = limited_overlays(&root, &[own.clone(), overlay.clone()], &earlier)?;
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0], own);
    assert_ne!(limited[1], overlay);
    assert_eq!(limited[1].parent(), Some(root.as_path()));
    let text = fs::read_to_string(&limited[1])?;
    assert_eq!(names(&text)?, ["web", "whoami"]);
    assert!(text.contains("GREETING: hi"), "{text}");
    assert_eq!(
        fs::metadata(&limited[1])?.permissions().mode() & 0o777,
        0o600
    );

    // The plan's own overlay is left as it was, and a second rollback in
    // the same operation limits it again.
    assert_eq!(
        names(&fs::read_to_string(&overlay)?)?,
        ["web", "whoami", "api"]
    );
    assert_eq!(
        limited_overlays(&root, std::slice::from_ref(&overlay), &earlier)?,
        [limited[1].clone()]
    );

    // An overlay that names only services the earlier files define is used
    // as it is.
    let every = BTreeSet::from(["web".to_owned(), "whoami".to_owned(), "api".to_owned()]);
    assert_eq!(
        limited_overlays(&root, std::slice::from_ref(&overlay), &every)?,
        [overlay]
    );
    Ok(())
}
