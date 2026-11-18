---
title: Coming from Docker Swarm
description: How Swarm concepts, commands, and Compose keys map to an mm cluster.
---

`mm` replaces Docker Swarm. It runs a Compose project across several hosts,
with encrypted networks and service names that span them, rolling updates
that roll back, HTTPS routes into every node, autoscaling, shared volumes, and
one merged log. The nodes run plain Docker Engine or Podman, and nothing runs
`docker swarm init`.

This page maps what you know to what you write in `mm`, lists the Compose keys
to change, and gives the order to move a project in. [Run a
cluster](./cluster.md) is the guide for the cluster itself.

## What works differently

- **No managers, workers, or quorum.** Every node is equal. Your machine, or
  CI, coordinates a deployment and talks to each node's `mmd`; each `mmd`
  changes only its own host. Losing a node never costs the others their
  ability to deploy.
- **Nothing is rescheduled.** A Swarm moves a failed node's tasks elsewhere.
  `mm` has no scheduler: a service runs on the nodes its `placement` names,
  and a node that goes down comes back with its own services. Place a service
  on more than one node if it has to survive one.
- **Every change is a plan.** `mm deploy plan` shows what each node will
  start, change, and stop, and `mm deploy apply` takes the hash of the plan
  you read.
- **No routing mesh and no virtual IPs.** A service name resolves to the
  addresses of its healthy containers, and a port is published only on the
  nodes that run the service. HTTP and HTTPS come in through an `ingress:`
  route, which reaches the service's containers on every node.
- **A failed update returns every node.** A rollout goes one node at a time,
  and when a container fails its health check the nodes already changed go
  back to what they ran before.

## Concepts and commands

| In a Swarm                                         | In mm                                                                                                                                                        |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `docker swarm init`, `join`, and join tokens       | List the hosts under `targets.<name>.nodes`, then run `mm target setup`. See [set up the nodes](./cluster.md#set-up-the-nodes).                              |
| Manager and worker roles                           | None. Every node is the same.                                                                                                                                |
| `docker node ls`                                   | `mm target nodes`                                                                                                                                            |
| Node labels and placement constraints              | `placement:` on the service: a node, a group from `cluster.groups`, `all`, or a list.                                                                        |
| `docker stack deploy`                              | `mm deploy plan`, then `mm deploy apply` with the plan's hash                                                                                                |
| A stack                                            | A stack: an entry under `stacks:` that names Compose files.                                                                                                  |
| A replicated service                               | `replicas:` on the service: a count of containers over its `placement`.                                                                                      |
| A global service                                   | `placement: all`                                                                                                                                             |
| `docker service scale`                             | Change `replicas` and deploy, or let `autoscale:` follow the load.                                                                                           |
| An overlay network                                 | A network under `networks:` in `mm.yaml`. It spans the nodes that run a service on it.                                                                       |
| An encrypted overlay network                       | Always on: traffic between nodes travels inside WireGuard.                                                                                                   |
| A service's virtual IP                             | None. The service name resolves to its healthy containers, the ones on the asking node first.                                                                |
| `tasks.<service>`                                  | The same name, with the same meaning: every healthy container of the service.                                                                                |
| A port published on every node by the routing mesh | For HTTP and HTTPS, an [`ingress:` route](./cluster.md#route-traffic-into-the-cluster): the ingress nodes reach the service on every node. Any other port is published on the nodes that run the service. |
| cloudflared and a hand-written proxy as services   | `ingress.type: cloudflared` on the environment: mm runs a connector next to each ingress node's proxy, writes its configuration from the routes, and opens no port. Route `headers`, `allow` lists, and `priority` replace hand-written proxy rules. See [Serve a cluster through Cloudflare Tunnel](./cloudflare-tunnel.md). |
| `update_config`                                    | `update: {order, parallelism, failure}` on the service in `mm.yaml`                                                                                          |
| `docker service update --rollback`                 | Automatic when a rollout fails. `mm deploy rollback` returns from one that finished.                                                                         |
| `docker service ls` and `docker service ps`        | `mm service status`                                                                                                                                          |
| `docker service logs`                              | `mm service logs`, merged across nodes in time order                                                                                                         |
| `docker secret` and a service's `secrets:`         | Secrets live in mm's secret store, or in OpenBao or Vault that [the nodes read](./cluster.md#read-secrets-on-the-nodes), and reach a service when it deploys. |
| `docker config` and a service's `configs:`         | [`config-files:`](./cluster.md#ship-configuration-files) on the service: each node writes the file and mounts it read-only.                                  |
| A volume on shared storage                         | `shared-volumes:` on the stack, for files that replicas share. Use local volumes for databases: see [the limits](./cluster.md#share-a-volume-between-nodes). |
| `docker node update --availability drain`          | Change the `placement` of the node's services and deploy.                                                                                                    |
| `docker service update --force`                    | `mm service restart`: one node at a time, each container healthy before the next.                                                                            |

## Compose keys

A Compose file written for a Swarm may place or multiply a service with keys
that only a Swarm reads. On a cluster target `mm config validate` and
`mm deploy plan` fail, and a finding `MM_COMPOSE_DEPLOY_KEY` for each key says
what to write:

| Compose key                                 | Write instead                                                                        |
| ------------------------------------------- | ------------------------------------------------------------------------------------ |
| `deploy.mode: global`                       | `placement: all` on the service in `mm.yaml`.                                        |
| `deploy.mode: replicated`                   | Nothing. Remove the key.                                                             |
| `deploy.replicas` above 1                   | `replicas:` on the service in `mm.yaml`, with `placement:` for its nodes.            |
| `deploy.placement` (constraints and others) | `placement:` with a node, a group from `cluster.groups`, `all`, or a list.           |
| `deploy.endpoint_mode`                      | Nothing. mm has no virtual IPs. Remove the key.                                      |

The same Compose file keeps working unchanged in an environment with one host:
Compose reads these keys there and `placement` is not involved.

Three more things to change by hand:

- **`deploy.update_config` and `deploy.rollback_config`** are not read by
  `mm`. Set `update:` on the service in `mm.yaml`; see [rolling deploys and
  rollback](../remote-daemon.md#rolling-deploys-and-rollback).
- **Networks with `driver: overlay`** cannot be created outside a Swarm.
  Remove the driver from the Compose file and declare the network under
  `networks:` in `mm.yaml`, then list it on the services that use it. See
  [connect services across nodes](./cluster.md#connect-services-across-nodes).
- **`depends_on` between services on different nodes** cannot be honoured by
  the engine. The plan names each one it does not keep
  (`depends_on not kept: app/web depends on db, which runs on db-1`); make the
  service tolerate a dependency that is not up yet, or order the stacks with
  `after:`.

## Move a project

1. Add `mm` to the project so that it deploys on one machine first:
   [add mm to a project](../onboarding.md). Your Compose files stay.
2. List the hosts under `nodes:` and give each service a `placement`. Run
   `mm config validate` and change each key it names, using the tables above.
3. Use new hosts, or free the existing ones: remove the stack and take each
   host out of the Swarm (`docker stack rm`, then `docker swarm leave --force`
   on every node) before you run `mm target setup`. A Swarm's own networks
   and `mm` should not manage the same engine.
4. Set up the nodes and deploy: [run a cluster](./cluster.md).
5. Move the data. `mm` creates its own volumes and does not adopt the ones a
   Swarm stack created. Restore each database from a dump or a backup into
   the new volume, and copy shared files into their shared volume.

With new hosts, keep the Swarm running until the new cluster serves traffic,
then move DNS.

## Projects that used the `docker-swarm` driver

`mm` 0.1.0 had a `docker-swarm` runtime driver, and development builds after
it added `members:` and `swarm:` to targets. Both are removed. To move such a
project:

- Set `runtime.driver` to `docker-compose`.
- List former `members:` under `targets.<name>.nodes`, each with a `name` and
  a `host`.
- Rename `targets.<name>.swarm:` to `cluster:`. `private-network.wireguard`
  becomes `mesh`, and `private-network.interface` becomes `mesh.interface`.
  `storage` and `groups` keep their names. Drop `address-pool` and
  `address-pool-mask`. `ingress` now lists the nodes that run the ingress
  proxy, written like a `placement`; leave it out for the first node.
- Delete `environments.<env>.tls`.
- In `placement:`, use a node name, a group name, `all`, or a list. `managers`
  and `any` are no longer accepted.

A configuration that still uses a removed key fails to load with a message
that names its replacement. The `mm target swarm` commands are gone:
`mm target nodes`, `mm target mesh status`, and `mm target storage status`
show the cluster.
