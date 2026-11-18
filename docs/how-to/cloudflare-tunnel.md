---
title: Serve a cluster through Cloudflare Tunnel
description: Take a cluster's HTTP and HTTPS routes through a Cloudflare Tunnel, with ports 80 and 443 closed on every node.
---

With `ingress.type: cloudflared` an environment's routes come in through a
[Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)
instead of ports 80 and 443. Every ingress node of the cluster runs a
connector (`cloudflared`) next to its ingress proxy. The connectors dial out
to Cloudflare, Cloudflare spreads the requests over them, and each connector
hands its requests to the proxy on its own node. The proxy routes them as
with public ingress: the plan reviews the routes, requests go to healthy
containers on any node, and a container that leaves is drained first. No
node opens a port for the web.

This guide creates the tunnel, stores its token, deploys, and checks that the
ports are closed.

## Requirements

- A cluster target that is set up: `mm target nodes` lists every node as
  ready. See [run a cluster](./cluster.md).
- A Cloudflare account with the zone of your domains, and `cloudflared` on
  your machine to create the tunnel.
- Outbound access from every ingress node to Cloudflare on port 7844, UDP
  and TCP. Nothing needs to reach the nodes.

## Create a locally-managed tunnel

Create the tunnel from your machine with `cloudflared`. A tunnel created this
way is locally managed: mm writes each connector's configuration from the
routes in `mm.yaml`. A tunnel created in the Cloudflare dashboard is managed
there and ignores what mm writes, so do not use one.

```sh
cloudflared tunnel login
cloudflared tunnel create shop-production
```

`create` prints the tunnel's ID. Then point each host the environment routes
at the tunnel. This adds a proxied DNS record in Cloudflare:

```sh
cloudflared tunnel route dns shop-production shop.example.com
cloudflared tunnel route dns shop-production api.example.com
```

## Configure the environment

Give the environment `ingress.type: cloudflared` and name the secret that
holds the tunnel's token:

```yaml
secrets:
  cloudflare.tunnel-token:
    storage: { provider: age, location: cloudflare.tunnel-token }

environments:
  production:
    target: production
    domains: { shop: shop.example.com, api: api.example.com }
    ingress:
      type: cloudflared
      cloudflared:
        token: cloudflare.tunnel-token

stacks:
  app:
    files: [compose.yaml]
    ingress:
      - { domain: shop, service: web }
      - { domain: api, service: api }
```

`type: cloudflared` without `cloudflared.token` fails to load and names the
`mm secret set` command to run. `cloudflared.image` replaces the connector's
image; by default mm runs a cloudflared release it pins by digest.

Cloudflare ends TLS, so the environment needs no certificate: nothing asks
Let's Encrypt, and `tls:` is refused. A route keeps `https: true`, and the
plan shows its `https://` address. To send plain HTTP visitors to HTTPS, turn
on **Always Use HTTPS** for the zone in Cloudflare.

## Store the token

Commit the change and plan once. The plan refuses with
`MM_TUNNEL_TOKEN_MISSING` while the secret has no value, and leaves the commit
on the nodes so that the secret can be set:

```sh
mm -e production deploy plan
cloudflared tunnel token shop-production | mm -e production secret set cloudflare.tunnel-token --stdin
```

`cloudflared tunnel token` prints the token, and `mm` seals it to every node.
Each ingress node writes it to a file only its connector can read; it never
appears in a process's arguments or environment, in Compose files, in
`docker inspect`, in plans, or in logs. On nodes that are not set up yet,
store it with `--local` and run `mm target setup` instead (see
[change an environment's variable](./change-a-variable.md)).

## Deploy

```sh
mm -e production deploy plan
```

The plan names the tunnel and, for each ingress node, the hosts its connector
sends to the node's proxy:

```text
  ingress: Cloudflare Tunnel 6ff42ae2-765d-4adf-8112-31c55c1551ef, token in secret cloudflare.tunnel-token; app-1, app-2, app-3 run a connector each, open no port, and send each request to a healthy container on the same node, else on any node
    app-1: the connector sends api.example.com, shop.example.com to this node's proxy and answers 404 to any other host
    app-2: the connector sends api.example.com, shop.example.com to this node's proxy and answers 404 to any other host
    app-3: the connector sends api.example.com, shop.example.com to this node's proxy and answers 404 to any other host
    https://api.example.com -> app/api port 8080
    https://shop.example.com -> app/web port 8080
```

Apply it with the hash the plan printed. The connectors change one node at a
time: each node starts its connector and waits until cloudflared reports a
connection to Cloudflare before the next node begins. A connector that does
not connect within 90 seconds fails the deployment on its node with
`MM_TUNNEL_NOT_CONNECTED`; that node goes back to the connector it ran
before, and the nodes after it are not changed.

## Check the connectors and the ports

```sh
mm -e production service status
mm -e production doctor
```

Under the services, the status says how many connections each node's
connector holds to Cloudflare; a working connector holds four:

```text
  node app-1: the tunnel connector holds 4 connections to Cloudflare
  node app-2: the tunnel connector holds 4 connections to Cloudflare
  node app-3: the tunnel connector holds 4 connections to Cloudflare
```

`mm doctor` reports `MM_TUNNEL_NOT_CONNECTED` for a connector that holds none
or does not answer, and names its log.

From a machine outside the cluster, check that the nodes take no web
traffic. Each command fails to connect:

```sh
curl --max-time 5 http://203.0.113.11/
curl --max-time 5 --insecure https://203.0.113.11/
```

Once every environment on the nodes uses the tunnel, close ports 80 and 443
in the provider's firewall too.

## What runs on each ingress node

- The ingress proxy, without ports 80 and 443 while no environment on the
  node uses `type: public`. It takes the tunnel's requests on
  `127.0.0.1:7955`.
- One connector per environment, in the proxy's Compose project
  (`mm-ingress`), in the node's network namespace. It runs as user 65532 with
  a read-only root, no capabilities, and no new privileges, reaches the proxy
  on `127.0.0.1:7955`, and answers its readiness on a port of `127.0.0.1`
  from 7960. Its configuration and token are under
  `/var/lib/mm/ingress/tunnels/`, the token readable by the connector's user
  alone (mode 0400).
- The configuration sends each host of the environment's routes to the proxy
  and answers 404 to every other host. Paths, `allow` lists, and headers are
  the proxy's job, as with public ingress.

## Limits

- Local development ignores `type`: `mm dev` serves the routes locally as
  always.
- A target of one host refuses `type: cloudflared`; it applies on a target
  that lists `nodes:`.
- HTTP and HTTPS routes, websockets included; no TCP or UDP routes.
- Another environment with `type: public` on the same node keeps ports 80
  and 443 open there. The plan says so for each node, and `mm doctor` warns
  with `MM_TUNNEL_PUBLIC_PORTS`. The tunnel's routes are never served on
  those ports.
- Stopping a node's connector, or the node, moves its traffic to the other
  connectors; requests in flight on that connector fail.
