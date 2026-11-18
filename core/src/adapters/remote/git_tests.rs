use std::collections::{BTreeMap, BTreeSet};
use std::process::Command as StdCommand;

use super::*;

fn git(directory: &Path, arguments: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(directory)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.test"])
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn enrolled(origin: &Path) -> EnrolledProject {
    EnrolledProject {
        name: "shop".to_owned(),
        origin: origin.display().to_string(),
        origin_host_key: None,
        delivery: Default::default(),
        environments: BTreeSet::from(["staging".to_owned()]),
        initial_deploy_from: BTreeMap::new(),
        ceiling: BTreeMap::new(),
        allow: Vec::new(),
    }
}

/// A bare origin with `main` (one commit) and `feature` (one more).
fn origin(root: &Path) -> Result<(PathBuf, String, String), Box<dyn std::error::Error>> {
    let work = root.join("work");
    std::fs::create_dir_all(work.join(".config"))?;
    git(&work, &["init", "--quiet", "--initial-branch=main"])?;
    std::fs::write(work.join(".config/mm.yaml"), "schema-version: 1\n")?;
    std::fs::write(work.join(".config/mm.overrides.yaml"), "{}\n")?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "main"])?;
    let main = git(&work, &["rev-parse", "HEAD"])?;
    git(&work, &["checkout", "--quiet", "-b", "feature"])?;
    std::fs::write(work.join("feature.txt"), "feature\n")?;
    std::fs::create_dir_all(work.join("deep/nested"))?;
    std::fs::write(work.join("deep/nested/file.txt"), "nested\n")?;
    std::os::unix::fs::symlink("../feature.txt", work.join("deep/link.txt"))?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "feature"])?;
    let feature = git(&work, &["rev-parse", "HEAD"])?;
    let bare = root.join("origin.git");
    git(
        root,
        &[
            "clone",
            "--quiet",
            "--bare",
            work.to_str().unwrap_or_default(),
            bare.to_str().unwrap_or_default(),
        ],
    )?;
    Ok((bare, main, feature))
}

#[tokio::test]
async fn fetches_checks_reachability_and_exports_immutable_checkouts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let (bare, main, feature) = origin(temporary.path())?;
    let project = ProjectId(uuid::Uuid::from_u128(5));
    let enrolled = enrolled(&bare);
    let repositories = ProjectRepositories::new(
        temporary.path().join("state"),
        temporary.path().join("projects"),
    );
    repositories.fetch(project, &enrolled).await?;
    repositories.fetch(project, &enrolled).await?;

    repositories
        .check_reachable(project, &enrolled, &feature, None)
        .await?;
    repositories
        .check_reachable(project, &enrolled, &main, Some(&["main".to_owned()]))
        .await?;
    let refused = repositories
        .check_reachable(project, &enrolled, &feature, Some(&["main".to_owned()]))
        .await;
    assert!(
        matches!(refused, Err(MmError::Security { .. })),
        "{refused:?}"
    );
    repositories
        .check_reachable(project, &enrolled, &feature, Some(&["*".to_owned()]))
        .await?;
    repositories
        .check_reachable(project, &enrolled, &feature, Some(&["feat*".to_owned()]))
        .await?;
    let unmatched = repositories
        .check_reachable(project, &enrolled, &feature, Some(&["*.x".to_owned()]))
        .await;
    assert!(
        matches!(unmatched, Err(MmError::Security { .. })),
        "{unmatched:?}"
    );
    assert!(
        repositories
            .check_reachable(project, &enrolled, &"f".repeat(40), None)
            .await
            .is_err()
    );
    assert!(
        repositories
            .check_reachable(project, &enrolled, "main", None)
            .await
            .is_err()
    );

    assert!(
        repositories
            .is_ancestor(project, &enrolled, &main, &feature)
            .await?
    );
    assert!(
        repositories
            .is_ancestor(project, &enrolled, &main, &main)
            .await?
    );
    assert!(
        !repositories
            .is_ancestor(project, &enrolled, &feature, &main)
            .await?
    );
    assert!(
        repositories
            .is_ancestor(project, &enrolled, &"f".repeat(40), &main)
            .await
            .is_err()
    );

    let root = repositories
        .checkout(project, &enrolled, "staging", &feature)
        .await?;
    assert!(root.ends_with(format!("staging/{}", &feature[..12])));
    assert!(root.join("feature.txt").is_file());
    assert!(root.join(".config/mm.yaml").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("deep/nested/file.txt"))?,
        "nested\n"
    );
    assert_eq!(
        std::fs::read_link(root.join("deep/link.txt"))?,
        PathBuf::from("../feature.txt")
    );
    assert!(
        !root.join(".config/mm.overrides.yaml").exists(),
        "overrides never reach a server"
    );
    assert_eq!(
        repositories
            .checkout(project, &enrolled, "staging", &feature)
            .await?,
        root
    );
    Ok(())
}

#[tokio::test]
async fn submodules_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let (bare, _, _) = origin(temporary.path())?;
    let work = temporary.path().join("work");
    git(&work, &["checkout", "--quiet", "main"])?;
    std::fs::write(
        work.join(".gitmodules"),
        "[submodule \"x\"]\n\tpath = x\n\turl = ../x\n",
    )?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "submodule"])?;
    git(
        &work,
        &["push", "--quiet", bare.to_str().unwrap_or_default(), "main"],
    )?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    let project = ProjectId(uuid::Uuid::from_u128(6));
    let enrolled = enrolled(&bare);
    let repositories = ProjectRepositories::new(
        temporary.path().join("state"),
        temporary.path().join("projects"),
    );
    repositories.fetch(project, &enrolled).await?;
    let refused = repositories
        .checkout(project, &enrolled, "staging", &commit)
        .await;
    assert!(refused.is_err_and(|error| error.to_string().contains("submodules")));
    assert_eq!(
        origin_host("git@github.com:acme/shop.git").as_deref(),
        Some("github.com")
    );
    assert_eq!(
        origin_host("ssh://git@git.example.test:2222/shop.git").as_deref(),
        Some("[git.example.test]:2222")
    );
    Ok(())
}

#[tokio::test]
async fn checkouts_hold_the_whole_tree_whatever_gitattributes_say()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let work = temporary.path().join("work");
    std::fs::create_dir_all(work.join("package/tests"))?;
    git(&work, &["init", "--quiet", "--initial-branch=main"])?;
    std::fs::write(
        work.join("package/.gitattributes"),
        "/tests export-ignore\nversion.txt export-subst\n",
    )?;
    std::fs::write(work.join("package/tests/case.txt"), "test\n")?;
    std::fs::write(work.join("package/version.txt"), "$Format:%H$\n")?;
    git(&work, &["add", "."])?;
    git(&work, &["commit", "--quiet", "-m", "package"])?;
    let commit = git(&work, &["rev-parse", "HEAD"])?;
    let bare = temporary.path().join("origin.git");
    git(
        temporary.path(),
        &[
            "clone",
            "--quiet",
            "--bare",
            work.to_str().unwrap_or_default(),
            bare.to_str().unwrap_or_default(),
        ],
    )?;
    let project = ProjectId(uuid::Uuid::from_u128(6));
    let enrolled = enrolled(&bare);
    let repositories = ProjectRepositories::new(
        temporary.path().join("state"),
        temporary.path().join("projects"),
    );
    repositories.fetch(project, &enrolled).await?;
    let root = repositories
        .checkout(project, &enrolled, "staging", &commit)
        .await?;
    assert_eq!(
        std::fs::read_to_string(root.join("package/tests/case.txt"))?,
        "test\n",
        "export-ignore does not drop files"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("package/version.txt"))?,
        "$Format:%H$\n",
        "export-subst does not rewrite files"
    );
    Ok(())
}
