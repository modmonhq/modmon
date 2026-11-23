---
title: Ship config files with a service
description: Mount a proxy configuration or an init script into a service, and replace only that service when the file changes.
---

A service often needs a file that is not a secret and not part of its image:
a proxy configuration, an init script, a settings file. List it under the
service's `config-files` and mm puts it on every host that runs the service
and mounts it read-only into the container. When the file changes, the next
deployment replaces the containers of the services that mount it and leaves
the others running.

## Declare a file

The key is the file's path in the container. `file` names a file in the
module's directory, shipped as it is:

```yaml
modules:
  proxy:
    path: proxy
    services:
      proxy:
        stack: app
        config-files:
          /etc/nginx/conf.d/site.conf: { file: site.conf }
```

Here `proxy/site.conf` in the project becomes `/etc/nginx/conf.d/site.conf`
in the `proxy` container. Commit the file: a remote host reads it from the
commit it deploys.

The file is owned by the user mm runs as on the host (root on a remote host)
and has mode `0444`, so every user in the container can read it and none can
change it. To change that, set a numeric owner or a mode:

```yaml
config-files:
  /etc/app/settings.toml:
    file: settings.toml
    uid: "101"
    gid: "101"
    mode: 0o440
```

`uid` and `gid` need mm to run as root on the host, which `mmd` does. On your
own machine, leave them out. On Docker Engine with `userns-remap`, containers
do not have the host's ids, and the plan refuses an owner other than root or
a mode not every user can read, as for [secret
files](./secret-delivery.md#file-delivery).

On hosts that run [Podman](./remote-daemon.md#run-podman-on-the-hosts), each
service has user ids of its own: `uid` and `gid` are the container's, and mm
gives the file to the host ids they are there (root's when left out). They
have to be from 0 to 65535; `mm config validate` refuses others with
`MM_USER_NAMESPACE`. A plain `userns_mode: auto` gets the service mm's own
ids as well. A service whose Compose file picks its own user namespace, such
as `userns_mode: auto:size=1024`, gets its ids from Podman only when its
container is created, so its files keep root's `uid` and `gid` and a mode
every user can read; remove `userns_mode`, or set it to plain `auto`, to give
them an owner.

## Render a file from bindings

Use `template` instead of `file` when the content differs between
environments. Every `{{ KEY }}` in the template is replaced by the
environment's [binding](./reference/configuration.md#bindings-environments-and-secrets)
of that key:

```nginx
# proxy/site.conf.tmpl
server {
  server_name {{ APP.HOST }};
  location / { proxy_pass http://{{ APP.UPSTREAM }}; }
}
```

```yaml
bindings:
  APP.UPSTREAM: { value: "web:8080" }
environments:
  production:
    bindings:
      APP.HOST: { value: shop.example.com }
modules:
  proxy:
    path: proxy
    services:
      proxy:
        stack: app
        config-files:
          /etc/nginx/conf.d/site.conf: { template: site.conf.tmpl }
```

A template also reads the [release facts](./reference/configuration.md#release-facts)
of the deployment, such as the image another module was built into:

```nginx
# worker/pool.conf.tmpl
worker_image = {{ MM.IMAGE.worker }}
release = {{ MM.RELEASE }}
```

The plan renders them; `mm config validate` renders them empty.

Nothing else in a template is special: there are no conditions, loops, or
escapes. A file that has to contain `{{` itself is shipped with `file`.

A template never takes a secret. A config file is kept on the host as it is,
and its content decides its version, so `mm config validate` refuses a
placeholder whose binding is a secret and says how to ship it instead: mount
the secret with [`secret-files`](./secret-delivery.md#file-delivery) and have
the configuration read `/run/secrets/<name>`.

## Check and deploy

```sh
mm config validate
mm deploy plan
```

`mm config validate` reads every file and renders every template for the
environment. It fails with `MM_CONFIG_FILE` when a file is missing, a path
leaves the module's directory, or a placeholder names no binding, and the
message says which entry and what to write.

The plan lists each config file with what it does to the service:

```text
  config file: app/proxy /etc/nginx/conf.d/site.conf from template proxy/site.conf.tmpl (version 3f2a9c1b04de): changed, app/proxy is replaced
  config file: app/web /etc/app/settings.toml from web/settings.toml (version 9d41c07e52aa): unchanged
```

- `new`: the service did not mount this file before.
- `changed`: the content, owner, or mode differs from what the last finished
  deployment mounted. The service's container is replaced. A changed binding
  counts when a template uses it.
- `unchanged`: the service keeps its container, unless something else about
  it changed.

On a cluster target each node renders the files of the services placed on
it, and the plan shows the lines in every node's section. The services roll
[one node at a time](./remote-daemon.md#rolling-deploys-and-rollback) like any
other change, and a service that mounts no changed file is not replaced on
any node.

## Versions on the host

mm keeps each version of a file under its own name on the host, so a running
container keeps the file it was started with until it is replaced. You never
rename a file to roll it out. When a deployment has finished, mm removes the
versions no service mounts any more. A deployment that fails and is rolled
back removes nothing, so the containers it returns to still find their files.

## Limits

- A config file is at most 1 MiB. Ship larger data in the image or a volume.
- The mount is one read-only file. A program that replaces its configuration
  file in place cannot do so, and a file that changes does not reach a
  running container: the container is replaced instead.
- Do not also mount something at the same path in the Compose file. The
  engine refuses two mounts at one path when it creates the container.
- Values come from `bindings` in the configuration and from the release
  being deployed. A value overridden on one machine with `mm env set` is not
  used in a template.
