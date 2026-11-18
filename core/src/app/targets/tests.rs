use super::*;
use std::path::Path;

#[test]
fn bootstrap_total_accounting_accepts_the_boundary_and_rejects_excess_and_overflow()
-> Result<(), MmError> {
    assert_eq!(
        checked_bootstrap_total(MAX_BOOTSTRAP_TOTAL_BYTES - 1, 1)?,
        MAX_BOOTSTRAP_TOTAL_BYTES
    );
    assert!(matches!(
        checked_bootstrap_total(MAX_BOOTSTRAP_TOTAL_BYTES, 1),
        Err(MmError::Target { .. })
    ));
    assert!(matches!(
        checked_bootstrap_total(usize::MAX, 1),
        Err(MmError::Target { .. })
    ));
    Ok(())
}

#[test]
fn ansible_input_classification_covers_extensions_variable_directories_and_identifiers() {
    for path in [
        ".config/ansible/bootstrap.yml",
        ".config/ansible/vars.yaml",
        ".config/ansible/data.json",
        ".config/ansible/group_vars/all",
        ".config/ansible/host_vars/node",
    ] {
        assert!(is_ansible_yaml_input(Path::new(path)), "{path}");
    }
    assert!(!is_ansible_yaml_input(Path::new(
        ".config/ansible/notes.txt"
    )));
    assert!(contains_identifier("q('file')", "q"));
    assert!(contains_identifier("(q)('file')", "q"));
    assert!(!contains_identifier("query", "q"));
    assert!(!contains_identifier("eq_value", "q"));
}

#[test]
fn ansible_config_accepts_only_the_closed_setting_model() -> Result<(), MmError> {
    let path = Path::new("ansible.cfg");
    reject_external_ansible_config(
        path,
        b"# comment\n; comment\n[defaults]\nhost_key_checking = TRUE\ninterpreter_python = auto_silent\nretry_files_enabled = False\npipelining = true\n",
    )?;
    assert!(matches!(
        reject_external_ansible_config(path, &[0xff]),
        Err(MmError::Target { .. })
    ));
    for source in [
        "not-a-setting\n",
        "inventory = inventory.yml\n",
        "host_key_checking = false\n",
        "interpreter_python = /usr/bin/python\n",
        "retry_files_enabled = true\n",
        "pipelining = maybe\n",
    ] {
        assert!(
            reject_external_ansible_config(path, source.as_bytes()).is_err(),
            "accepted {source:?}"
        );
    }
    Ok(())
}

#[test]
fn ansible_yaml_rejects_transitive_loading_templating_tags_and_invalid_documents() {
    let path = Path::new("bootstrap.yml");
    for source in [
        "not: [valid\n",
        "--- !unsafe tagged\n",
        "---\n- import_playbook: other.yml\n",
        "---\n- with_items: []\n",
        "---\n- ansible_private_key_file: /tmp/key\n",
        "---\n- community.general.some_module: {}\n",
        "---\n- debug:\n    msg: '{{ unsafe }}'\n",
        "---\n- debug:\n    msg: '{% unsafe %}'\n",
        "---\n- debug:\n    msg: '{# unsafe #}'\n",
        "---\n- debug:\n    msg: '#jinja2 unsafe'\n",
        "---\n- debug:\n    msg: 'ansible_ssh_private_key_file'\n",
        "---\n- debug:\n    msg: 'ProxyCommand outside'\n",
        "---\n- debug:\n    msg: 'private_key outside'\n",
        "---\n- debug:\n    msg: lookup('file', 'outside')\n",
        "---\n- debug:\n    msg: query('file', 'outside')\n",
        "---\n- debug:\n    msg: q('file', 'outside')\n",
    ] {
        assert!(
            reject_transitive_ansible_inputs(path, source.as_bytes()).is_err(),
            "accepted {source:?}"
        );
    }
}

#[test]
fn ansible_yaml_accepts_reviewed_builtin_shapes_and_safe_expressions() -> Result<(), MmError> {
    let source = r#"
---
- hosts: mm_targets
  tasks:
    - ansible.builtin.debug:
        msg:
          - "{{ ansible_user }}"
          - "{{ item.path }}"
          - "{{ item.mode }}"
          - "{{ ansible_distribution_release }}"
          - "{{ {'x86_64':'amd64','aarch64':'arm64'}[ansible_architecture] }}"
          - "ansible_distribution == 'Debian'"
          - "ansible_distribution == 'Ubuntu'"
          - "ansible_distribution in ['Debian', 'Ubuntu']"
          - 1
          - true
"#;
    reject_transitive_ansible_inputs(Path::new("bootstrap.yml"), source.as_bytes())
}

#[test]
fn bootstrap_discovery_rejects_markers_excess_files_symlinks_and_special_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let marker = crate::test_support::tempdir()?;
    let directory = marker.path().join(".config/ansible");
    fs::create_dir_all(&directory)?;
    fs::write(directory.join(".mm-initializing"), "incomplete")?;
    assert!(matches!(
        discover_bootstrap_files(marker.path()),
        Err(MmError::Target { .. })
    ));

    let excess = crate::test_support::tempdir()?;
    let directory = excess.path().join(".config/ansible");
    fs::create_dir_all(&directory)?;
    for index in 0..=MAX_BOOTSTRAP_INPUT_FILES {
        fs::write(directory.join(format!("input-{index:03}.txt")), [])?;
    }
    assert!(matches!(
        discover_bootstrap_files(excess.path()),
        Err(MmError::Target { .. })
    ));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        use std::os::unix::net::UnixListener;

        let symlink_root = crate::test_support::tempdir()?;
        let directory = symlink_root.path().join(".config/ansible");
        fs::create_dir_all(&directory)?;
        symlink("outside", directory.join("linked"))?;
        assert!(matches!(
            discover_bootstrap_files(symlink_root.path()),
            Err(MmError::Security { .. })
        ));

        let special_root = crate::test_support::tempdir()?;
        let directory = special_root.path().join(".config/ansible");
        fs::create_dir_all(&directory)?;
        let _listener = UnixListener::bind(directory.join("socket"))?;
        assert!(matches!(
            discover_bootstrap_files(special_root.path()),
            Err(MmError::Security { .. })
        ));
    }
    Ok(())
}

#[test]
fn bootstrap_collection_requires_starter_files_and_bounds_each_input()
-> Result<(), Box<dyn std::error::Error>> {
    let missing = crate::test_support::tempdir()?;
    fs::create_dir_all(missing.path().join(".config/ansible"))?;
    assert!(matches!(
        collect_bootstrap_inputs(missing.path()),
        Err(MmError::Target { .. })
    ));

    let oversized = crate::test_support::tempdir()?;
    let directory = oversized.path().join(".config/ansible");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("ansible.cfg"),
        "[defaults]\nhost_key_checking = True\n",
    )?;
    fs::write(
        directory.join("bootstrap.yml"),
        vec![b'x'; MAX_BOOTSTRAP_INPUT_BYTES + 1],
    )?;
    assert!(collect_bootstrap_inputs(oversized.path()).is_err());
    Ok(())
}
