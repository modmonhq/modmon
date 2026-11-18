use super::*;

fn rewritten(source: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = absolute_file_references(
        Path::new("compose.yaml"),
        source.as_bytes().to_vec(),
        Path::new("/srv/project"),
    )?;
    Ok(serde_yaml_ng::from_slice(&bytes)?)
}

#[test]
fn relative_references_become_absolute_under_the_project_directory()
-> Result<(), Box<dyn std::error::Error>> {
    let model = rewritten(
        r"
services:
  db:
    image: postgres
    volumes:
      - data:/var/lib/postgresql
      - ./init/init.sql:/docker-entrypoint-initdb.d/init.sql:ro
      - {type: bind, source: ../shared, target: /shared}
      - /abs:/abs
    env_file: [./a.env, {path: ../b.env, required: false}]
    configs: [{source: init, target: /init.sql}]
configs:
  init: {file: ./infrastructure/postgres/init.sql}
secrets:
  token: {file: secrets/token.txt}
",
    )?;
    let volumes = &model["services"]["db"]["volumes"];
    assert_eq!(volumes[0], Value::from("data:/var/lib/postgresql"));
    assert_eq!(
        volumes[1],
        Value::from("/srv/project/init/init.sql:/docker-entrypoint-initdb.d/init.sql:ro")
    );
    assert_eq!(volumes[2]["source"], Value::from("/srv/shared"));
    assert_eq!(volumes[3], Value::from("/abs:/abs"));
    let env_file = &model["services"]["db"]["env_file"];
    assert_eq!(env_file[0], Value::from("/srv/project/a.env"));
    assert_eq!(env_file[1]["path"], Value::from("/srv/b.env"));
    assert_eq!(
        model["configs"]["init"]["file"],
        Value::from("/srv/project/infrastructure/postgres/init.sql")
    );
    assert_eq!(
        model["secrets"]["token"]["file"],
        Value::from("secrets/token.txt"),
        "only ./ and ../ paths are rewritten"
    );
    Ok(())
}

#[test]
fn files_without_relative_references_keep_their_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let source = "# comment kept\nservices:\n  web: {image: nginx, volumes: [cache:/cache]}\n";
    let bytes = absolute_file_references(
        Path::new("compose.yaml"),
        source.as_bytes().to_vec(),
        Path::new("/srv/project"),
    )?;
    assert_eq!(bytes, source.as_bytes());
    Ok(())
}
