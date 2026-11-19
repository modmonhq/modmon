//! Which CPU the `mm` and `mmd` that `mm target setup` installs are built
//! for, read from their ELF headers.

use mm::domain::target::{HOST_ARCHITECTURES, linux_architecture};

fn elf(class: u8, data: u8, machine: u16) -> Vec<u8> {
    let mut header = vec![0x7f, b'E', b'L', b'F', class, data, 1, 0];
    header.resize(18, 0);
    header.extend_from_slice(&machine.to_le_bytes());
    header.resize(64, 0);
    header
}

#[test]
fn linux_executables_name_their_cpu_and_anything_else_says_what_it_is() {
    assert_eq!(HOST_ARCHITECTURES, ["x86_64", "aarch64"]);
    assert_eq!(linux_architecture(&elf(2, 1, 62)), Ok("x86_64"));
    assert_eq!(linux_architecture(&elf(2, 1, 183)), Ok("aarch64"));
    let riscv = linux_architecture(&elf(2, 1, 243))
        .err()
        .unwrap_or_default();
    assert!(riscv.contains("ELF machine 243"), "{riscv}");
    for other in [elf(1, 1, 62), elf(2, 2, 62)] {
        let refused = linux_architecture(&other).err().unwrap_or_default();
        assert!(refused.contains("64-bit little-endian"), "{refused}");
    }
    for macos in [
        vec![0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1],
        vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2],
    ] {
        assert_eq!(
            linux_architecture(&macos),
            Err("is built for macOS, not Linux".to_owned())
        );
    }
    for other in [
        b"#!/bin/sh\n".to_vec(),
        Vec::new(),
        elf(2, 1, 62)[..12].to_vec(),
    ] {
        assert!(linux_architecture(&other).is_err());
    }
}

#[test]
fn the_access_playbook_refuses_a_host_of_another_cpu_before_it_installs_anything()
-> Result<(), Box<dyn std::error::Error>> {
    let playbook: serde_yaml_ng::Value = serde_yaml_ng::from_str(include_str!(
        "../src/adapters/target/ansible_bootstrap/access.yml"
    ))?;
    let tasks = playbook[0]["tasks"]
        .as_sequence()
        .ok_or("the access playbook has no tasks")?;
    let name = |task: &serde_yaml_ng::Value| task["name"].as_str().unwrap_or_default().to_owned();
    let check = tasks
        .iter()
        .position(|task| name(task) == "Require mm and mmd built for this host's CPU")
        .ok_or("no task checks the host's CPU")?;
    let first_change = tasks
        .iter()
        .position(|task| task.get("ansible.builtin.assert").is_none())
        .ok_or("the playbook changes nothing")?;
    assert!(check < first_change);
    let condition = format!("{:?}", tasks[check]["ansible.builtin.assert"]["that"]);
    assert!(condition.contains("mm_access_architecture"), "{condition}");
    Ok(())
}
